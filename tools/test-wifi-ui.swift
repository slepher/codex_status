// Native macOS rendering regression. No device, token, network or Rust IPC access.
// Compile: swiftc tools/test-wifi-ui.swift -o artifacts/bridge-macos/wifi-ui/test-wifi-ui
// Run: <binary> <production index.html> <scenario JS> <output directory>
import AppKit
import WebKit

final class Runner: NSObject, WKNavigationDelegate {
    let web: WKWebView
    let window: NSWindow
    let output: URL
    let scenarios: String
    init(html: URL, script: URL, output: URL) throws {
        self.output = output
        scenarios = try String(contentsOf: script, encoding: .utf8)
        let config = WKWebViewConfiguration()
        let bootstrap = """
        window.__uiCalls=[];window.__uiErrors=[];
        window.addEventListener('error',e=>window.__uiErrors.push(e.message));
        window.addEventListener('unhandledrejection',e=>window.__uiErrors.push(String(e.reason)));
        window.setInterval=()=>0;
        window.__TAURI__={core:{invoke:async(name,args)=>{
          if(window.__wifiRoundtripInvoke)return window.__wifiRoundtripInvoke(name,args);
          window.__uiCalls.push({name,args});
          if(name==='platform_wifi'&&window.__wifiTestState){
            if(args.op==='save'){if(window.__wifiSaveError)throw window.__wifiSaveError;window.__wifiTestState.targets=structuredClone(args.body.targets);window.__wifiTestState.local_revision++;return structuredClone(window.__wifiTestState);}
            if(args.op==='view')return {wifi:structuredClone(window.__wifiTestState),differences:[],host:{state:'permission_required'}};
          }
          return {devices:[],profiles:[],templates:[],sources:[],jobs:[],plans:[],contexts:[]};
        }}};
        """
        config.userContentController.addUserScript(WKUserScript(source: bootstrap, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        web = WKWebView(frame: NSRect(x:0,y:0,width:560,height:680),configuration:config)
        window = NSWindow(contentRect:web.frame,styleMask:[.titled,.closable,.resizable],backing:.buffered,defer:false)
        super.init()
        web.navigationDelegate=self
        window.title="Bridge Wi-Fi UI rendering test · synthetic data"
        window.contentView=web
        window.makeKeyAndOrderFront(nil)
        web.loadFileURL(html,allowingReadAccessTo:html.deletingLastPathComponent())
    }
    func navigationDidFail(_ error: Error) { fputs("UI TEST FAILED: \(error)\n",stderr); exit(1) }
    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) { navigationDidFail(error) }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) { navigationDidFail(error) }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Task { @MainActor in
            do {
                try await Task.sleep(nanoseconds: 300_000_000)
                _ = try await web.evaluateJavaScript(scenarios + ";true;")
                var results: [Any] = []
                for (width,height) in [(560,680),(1000,800)] {
                    window.setContentSize(NSSize(width:width,height:height))
                    for scenario in ["ble-recovery","empty","bridge","device","compare","waiting","error","permission-request","permission-settings","current-missing","current-connecting","current-error","current-ready","rename","delete","current-unreachable","sync-guard","imported-edit","save-error","delivery-alignment","roundtrip-own","roundtrip-other","online-usb","online-boot","stale-cache","job-feedback","job-feedback-error","job-blocked","failure-sent","failure-unknown","failure-network","failure-budget","endpoint-rejected","endpoint-unknown"] {
                        let result = try await web.callAsyncJavaScript("return await window.__wifiUiScenario(name)",arguments:["name":scenario],in:nil,contentWorld:.page)
                        results.append(result ?? NSNull())
                        let config=WKSnapshotConfiguration()
                        config.rect=web.bounds
                        let image = try await web.takeSnapshot(configuration:config)
                        let bitmap = NSBitmapImageRep(data:image.tiffRepresentation!)!
                        try bitmap.representation(using:.png,properties:[:])!.write(to:output.appendingPathComponent("\(width)-\(scenario).png"))
                        if scenario == "bridge" {
                            _ = try await web.evaluateJavaScript("document.getElementById('wifi-editor').scrollIntoView({block:'start'});true;")
                            try await Task.sleep(nanoseconds:100_000_000)
                            let editor = try await web.takeSnapshot(configuration:config)
                            let editorBitmap=NSBitmapImageRep(data:editor.tiffRepresentation!)!
                            try editorBitmap.representation(using:.png,properties:[:])!.write(to:output.appendingPathComponent("\(width)-editor.png"))
                        }
                        print("PASS \(width)x\(height) \(scenario)")
                    }
                }
                let data=try JSONSerialization.data(withJSONObject:results,options:[.prettyPrinted,.sortedKeys])
                try data.write(to:output.appendingPathComponent("results.json"))
                print("Native WKWebView: \(results.count) scenarios passed; screenshots and geometry saved.")
                exit(0)
            } catch { navigationDidFail(error) }
        }
    }
}
guard CommandLine.arguments.count == 4 else { fatalError("Expected HTML, scenario JS, output directory") }
let app=NSApplication.shared
app.setActivationPolicy(.regular)
let runner=try Runner(html:URL(fileURLWithPath:CommandLine.arguments[1]),script:URL(fileURLWithPath:CommandLine.arguments[2]),output:URL(fileURLWithPath:CommandLine.arguments[3]))
Timer.scheduledTimer(withTimeInterval:60,repeats:false){_ in fputs("UI TEST FAILED: 60s timeout\n",stderr);exit(1)}
app.run()
