"""Execute the complete native 0041 candidate with explicit QEMU peripherals.
WSL-only. Imports frozen sibling tooling read-only; no device or LAN traffic.
Socket substitution is restricted by that runner to callers in our RV32 module.
"""
from pathlib import Path
import importlib.util,json,sys,socket,struct,threading,time,hashlib,queue,secrets
ROOT=Path(__file__).resolve().parents[1]
NATIVE_RUNNER_SHA=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
FIRMWARE=ROOT.parent/'cuktech-ap01-firmware'
sys.path.insert(0,str(FIRMWARE/'tooling/scripts'))
synthetic_mac=None
transition_probe='--transition-probe' in sys.argv
if transition_probe:sys.argv.remove('--transition-probe')
host_rebuild_only='--host-rebuild-only' in sys.argv
if host_rebuild_only:sys.argv.remove('--host-rebuild-only')
host_lifecycle_probe='--host-lifecycle-probe' in sys.argv
if host_lifecycle_probe:sys.argv.remove('--host-lifecycle-probe')
assert not host_rebuild_only or host_lifecycle_probe
screen_host_probe_only='--screen-host-probe-only' in sys.argv
if screen_host_probe_only:sys.argv.remove('--screen-host-probe-only')
assert not screen_host_probe_only or transition_probe
bundle_replace_probe='--bundle-replace-probe' in sys.argv
if bundle_replace_probe:sys.argv.remove('--bundle-replace-probe')
animation_wexti_model='--animation-wexti-model' in sys.argv
if animation_wexti_model:sys.argv.remove('--animation-wexti-model')
model_epoch=None
if '--model-wall-epoch' in sys.argv:
    i=sys.argv.index('--model-wall-epoch');model_epoch=[int(sys.argv[i+1])];del sys.argv[i:i+2]
input_model='--lvgl-input-model' in sys.argv
if input_model:sys.argv.remove('--lvgl-input-model')
fb_ready_model='--fb-ready-api-model' in sys.argv
if fb_ready_model:sys.argv.remove('--fb-ready-api-model')
assert not fb_ready_model or input_model,'framebuffer fixture requires the actual UI-thread gate'
resume_report=None
if '--resume-native-report' in sys.argv:
    i=sys.argv.index('--resume-native-report');resume_report=Path(sys.argv[i+1]);del sys.argv[i:i+2]
    assert resume_report.parent.resolve()==(ROOT/'artifacts/ap01-platform-integration/device').resolve()
native_bundle=ROOT/'artifacts/ap01-platform-integration/device/native-app-bundle.json'
if '--native-bundle' in sys.argv:
    i=sys.argv.index('--native-bundle');native_bundle=Path(sys.argv[i+1]);del sys.argv[i:i+2]
firmware_probe='--firmware-probe' in sys.argv
if firmware_probe:sys.argv.remove('--firmware-probe')
unconfigured_slots='--unconfigured-slots' in sys.argv
if unconfigured_slots:sys.argv.remove('--unconfigured-slots')
firmware_only='--firmware-only' in sys.argv
if firmware_only:sys.argv.remove('--firmware-only')
assert not firmware_only or firmware_probe and resume_report
trial_cut='--boot-trial-cut' in sys.argv
if trial_cut:sys.argv.remove('--boot-trial-cut')
fallback_receipt='--fallback-receipt' in sys.argv
if fallback_receipt:sys.argv.remove('--fallback-receipt')
firmware_receipt=None
slot_probe='--slot-probe' in sys.argv
if slot_probe:sys.argv.remove('--slot-probe')
if '--firmware-receipt' in sys.argv:
    i=sys.argv.index('--firmware-receipt');firmware_receipt=Path(sys.argv[i+1]);del sys.argv[i:i+2]
if '--model-wifi-mac' in sys.argv:
    i=sys.argv.index('--model-wifi-mac');synthetic_mac=bytes.fromhex(sys.argv[i+1]);del sys.argv[i:i+2]
    assert len(synthetic_mac)==6 and any(synthetic_mac) and not synthetic_mac[0]&1
runner_variant_sha=None
def module(name,path):
    global runner_variant_sha
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec)
    if name=='ap01_frozen_ota_runner' and (synthetic_mac or input_model):
        source=path.read_text(encoding='utf-8')
        if synthetic_mac or model_epoch:
            needle="name == 'socket' or args6[0]";assert source.count(needle)==1
            source=source.replace(needle,"name in ('socket','wifi_mac','wall_epoch') or args6[0]")
        if input_model:
            # Full application/recovery probes include many bounded requests.
            # Extend only this private wrapper's wall-clock cap; frozen runner
            # sources remain unchanged and their exact variant is fingerprinted.
            source=source.replace('1 <= args.timeout <= 240','1 <= args.timeout <= 900')
            source=source.replace('1 <= args.safety_seconds <= 240','1 <= args.safety_seconds <= 840')
            for needle,replacement in [
                ("completed = done.is_set() and (upload is None or report.get('client_error'))",
                 "completed = report.get('native_probe_finished') or done.is_set() and (upload is None or report.get('client_error'))"),
                ("if done.is_set() and (upload is None or report.get('client_error')):",
                 "if report.get('native_probe_finished') or done.is_set() and (upload is None or report.get('client_error')):")]:
                assert source.count(needle)==1;source=source.replace(needle,replacement)
        runner_variant_sha=hashlib.sha256(source.encode()).hexdigest()
        exec(compile(source,str(path),'exec'),m.__dict__)
    else:spec.loader.exec_module(m)
    return m
base=module('ap01_frozen_ota_runner',FIRMWARE/'tooling/scripts/run-qemu-ota-channel.py')
builder=module('ap01_native_builder',ROOT/'tools/build-ap01-rom.py')
def linux(path):
    text=str(path).replace('\\','/')
    return '/mnt/'+text[0].lower()+text[2:] if len(text)>2 and text[1]==':' else text
class NativeBuilder:
    @staticmethod
    def validate_candidate(data,path):
        m=json.loads(Path(path).read_text(encoding='utf-8'))
        for key in ('original','payload','image'):m[key]['path']=linux(m[key]['path'])
        payload=Path(m['payload']['path']).read_bytes();original=Path(m['original']['path']).read_bytes()
        if builder.sha(payload)!=m['payload']['sha256'] or data!=builder.assemble(original,payload,int(m['payload']['entry'],16),m.get('unsigned_ota',False),int(m['timer_delete_guard']['entry'],16) if m.get('timer_delete_guard') else None):
            raise ValueError('native candidate reconstruction mismatch')
        return m
profile=dict(base.PROFILES['ap01-ota-channel-0041-v1'])
profile['apis']=dict(profile['apis']);profile['apis'][0xa003942c]='recvfrom'
if synthetic_mac:profile['apis'][0xa005f240]='wifi_mac'
if model_epoch:profile['apis'][0xa019fc3e]='wall_epoch'
if animation_wexti_model:profile['apis'][0xa008a876]='animation_wexti_instruction'
base.builder_for=lambda path:(NativeBuilder,builder.FORMAT,profile)
OriginalProvider=base.Provider
OriginalRSP=base.BufferedRSP
debug_snapshots=[]
native_provider=None
native_ui_root=None
native_runtime=None
native_qmp=None
input_actions=queue.Queue()
injected=None
input_trace=[]
manifest=json.loads(Path(sys.argv[sys.argv.index('--manifest')+1]).read_text(encoding='utf-8'))
assert not host_lifecycle_probe or manifest.get('screen_host_probe'), 'Requires an opt-in probe build'

def partition_tables(data):
    import zlib
    tables=[]
    for offset in (0xe000,0xf000):
        p=data[offset:offset+596];count=int.from_bytes(p[6:8],'little')
        if p[:4]!=b'BFPT' or not 1<=count<=16 or zlib.crc32(p[:12])!=int.from_bytes(p[12:16],'little'):continue
        entries=p[16:16+count*36]
        if zlib.crc32(entries)!=int.from_bytes(p[16+count*36:20+count*36],'little'):continue
        fw=next(entries[i:i+36] for i in range(0,len(entries),36) if entries[i]==0)
        tables.append({'offset':hex(offset),'age':int.from_bytes(p[8:12],'little'),'slot':fw[2]})
    assert tables,'no valid partition table'
    return sorted(tables,key=lambda t:t['age'])
timer_va=int(manifest.get('native_abi',{}).get('ui_timer','0'),16)
send_event_va=int(manifest.get('native_abi',{}).get('lv_obj_send_event','0'),16)
class NativeRSP(OriginalRSP):
    def ui_state(self):
        def read(address,count):return bytes.fromhex(super(NativeRSP,self).request(f'm{address:x},{count:x}'))
        global native_ui_root
        offsets=manifest['native_abi']['runtime_offsets']
        native_ui_root=int.from_bytes(read(native_runtime+offsets['root'],4),'little')
        if not native_ui_root:return {'root_ready':0,'page':-1,'focused':0,'generation':0,'host_phase':0}
        state={'root_ready':read(native_ui_root+0x4c,1)[0],
               'page':int.from_bytes(read(native_ui_root+0x34,4),'little'),
               'focused':read(native_runtime+offsets['focused'],1)[0],
               'generation':int.from_bytes(read(native_runtime+offsets['shownGeneration'],4),'little')}
        for name in ('content','image'):
            pointer=int.from_bytes(read(native_runtime+offsets[name],4),'little')
            state[name]=pointer
            if pointer:
                state[name+'_hidden']=bool(int.from_bytes(read(pointer+0x24,4),'little')&1)
                state[name+'_coords']=list(struct.unpack('<4i',read(pointer+0x14,16)))
        if manifest.get('screen_host'):
            host=manifest['screen_host'];state['host_phase']=int.from_bytes(read(native_runtime+host['host_offset']+host['phase_offset'],4),'little')
        return state
    def request(self,packet):
        global native_ui_root,native_runtime,injected
        result=super().request(packet)
        if packet=='g' and len(result)>=264:
            regs=struct.unpack_from('<33I',bytes.fromhex(result));pc=regs[32]
            if animation_wexti_model and pc==0xa008a876:
                # The frozen CPU lacks this observed RV32 P instruction.
                # Execute only WEXTI a1,s6,22; leave the native curve, window
                # positions, timer and completion callback running unchanged.
                assert super().request('ma008a876,4')=='f7056bdf'
                value=(((regs[23]<<32)|regs[22])>>22)&0xffffffff
                super().request(f'Pb={value.to_bytes(4,"little").hex()}')
                super().request('P20=7aa808a0')
                if native_provider:
                    native_provider.report['animation_wexti_model_steps']=native_provider.report.get('animation_wexti_model_steps',0)+1
                super().request('c')
                return self.request('g')
            if injected and injected[1].get('host_probe')==3 and pc in (0xa008ae58,0xa0089d02):
                if (pc==0xa008ae58 and regs[10]==0) or (pc==0xa0089d02 and regs[11]==0):
                    item={'pc':hex(pc),'a0':hex(regs[10]),'a1':hex(regs[11]),'ra':hex(regs[1]),'sp':hex(regs[2])}
                    item['stack']=super().request(f'm{regs[2]:x},100')
                    native_provider.report.setdefault('null_delete_arguments',[]).append(item)
                super().request(f'z0,{pc:x},2');super().request('s');super().request(f'Z0,{pc:x},2');super().request('c')
                return self.request('g')
            if injected and injected[1].get('rebuild_original') and pc in (0xa005d1a0,0xa005cdc2):
                saved,action=injected;injected=None
                super().request('z0,a005d1a0,2');super().request('z0,a005cdc2,2')
                action['value']=pc==0xa005d1a0
                self.ui_state()
                for register in range(1,33):
                    if saved[register]!=regs[register]:super().request(f'P{register:x}={saved[register].to_bytes(4,"little").hex()}')
                action['done'].set();return struct.pack('<33I',*saved).hex()
            if native_provider:
                for call in native_provider.report.get('native_ff13_calls',[]):
                    if 'result' in call and 'guest_uptime_ms' not in call:
                        call['guest_uptime_ms']=int.from_bytes(bytes.fromhex(super().request(f"m{profile['tick']:x},4")),'little')
            if native_provider and native_qmp and not native_provider.report.get('native_boot_prearmed'):
                calls=native_provider.report.get('native_ff13_calls',[])
                if calls and calls[0].get('result')==0 and '--active-slot' in sys.argv and sys.argv[sys.argv.index('--active-slot')+1]=='1':
                    path=Path(sys.argv[sys.argv.index('--output')+1]).with_suffix('.trial-armed.flash.bin').resolve()
                    native_qmp.command('qom-set',{'path':'/machine','property':'flash-snapshot','value':str(path)})
                    tables=partition_tables(path.read_bytes());assert tables[-1]['slot']==0
                    native_provider.report['native_boot_prearmed']={'path':str(path),'tables':tables,'ff13_calls':len(calls),'guest_uptime_ms':calls[0]['guest_uptime_ms']}
                    if trial_cut:
                        native_provider.report['native_boot_trial_cut']=True
                        native_provider.report['native_probe_finished']=True
            if pc==0xa005d180:native_ui_root=regs[10]
            if pc==0xa0080da6 and native_provider:
                native_provider.report['native_display_refresh_calls']=native_provider.report.get('native_display_refresh_calls',0)+1
            if input_model and pc==timer_va:
                if native_runtime is None:native_runtime=regs[10] if manifest.get('screen_host',{}).get('timer_context_direct') else int.from_bytes(bytes.fromhex(super().request(f'm{regs[10]+12:x},4')),'little')
                if injected:
                    saved,action=injected;injected=None
                    if action.get('host_probe') is not None:
                        if action['host_probe']==3:
                            super().request('z0,a008ae58,2');super().request('z0,a0089d02,2')
                        action['value']=regs[10];self.ui_state()
                    if action.get('sample'):action['value']=self.ui_state()
                    if action.get('calendar') is not None:
                        action['value']=list(struct.unpack('<9i',bytes.fromhex(super().request(f"m{action['calendar_output']:x},24"))))
                    if action.get('wall_time'):action['value']=regs[10]
                    for register in range(1,33):
                        if saved[register]!=regs[register]:super().request(f'P{register:x}={saved[register].to_bytes(4,"little").hex()}')
                    action['done'].set();input_trace.append({'event':action.get('event','framebuffer_ready'),'key':action.get('key',0),'completed':True})
                    return struct.pack('<33I',*saved).hex()
                action=None
                if fb_ready_model:
                    # Honor an armed peripheral event even when a probe is
                    # polling UI state. Dequeuing probes first starves POLLOUT
                    # and prevents real LVGL draw callbacks indefinitely.
                    context=int.from_bytes(bytes.fromhex(super().request('m62fc961c,4')),'little')
                    if context and super().request(f'm{context+0x5c:x},1')=='01':
                        action={'framebuffer_ready':context+0x60,'done':threading.Event()}
                if action is None:
                    try:action=input_actions.get_nowait()
                    except queue.Empty:pass
                if action and native_runtime:
                    if action.get('nor_snapshot'):
                        native_qmp.command('qom-set',{'path':'/machine','property':'flash-snapshot','value':action['nor_snapshot']});action['done'].set()
                    elif action.get('snapshot'):
                        path=Path(action['snapshot'])
                        native_qmp.command('screendump',{'filename':str(path)})
                        for index in (0,1):
                            path.with_suffix(f'.fb{index}.rgb565').write_bytes(native_provider.read(0xa8000020+153600*index,153600))
                        image_offset=manifest['native_abi']['runtime_offsets']['image']
                        object_address=int.from_bytes(native_provider.read(native_runtime+image_offset,4),'little')
                        if object_address:
                            source=int.from_bytes(native_provider.read(object_address+0x2c,4),'little')
                            if source:
                                descriptor=native_provider.read(source,24)
                                if int.from_bytes(descriptor[12:16],'little')==38464:
                                    data=int.from_bytes(descriptor[16:20],'little')
                                    path.with_suffix('.palette4').write_bytes(native_provider.read(data+64,38400))
                        action['done'].set()
                    elif action.get('observe'):
                        action['value']=self.ui_state();action['done'].set()
                    elif action.get('onboarded'):
                        super().request('M62fc4038,1:01');action['done'].set()
                    else:
                        saved=list(regs);temporary=(regs[2]-128)&~15
                        param=temporary+64
                        super().request(f'M{param:x},4:{struct.pack("<I",action.get("key",0)).hex()}')
                        if action.get('rebuild_original'):
                            # Replay the exact original inlined theme constructor,
                            # excluding lv_init/fb/input setup and the UI main loop.
                            temporary=(regs[2]-1152)&~15;frame=temporary+1024
                            super().request(f'M{temporary:x},400:'+bytes(1024).hex())
                            super().request(f'M{frame-964:x},4:'+struct.pack('<I',0x62fc957c).hex())
                            assert super().request('Z0,a005d1a0,2')=='OK'
                            assert super().request('Z0,a005cdc2,2')=='OK'
                            changes={1:timer_va,2:temporary,8:frame,9:0xa01c4000,18:0xa01c4000,32:0xa005cd68}
                        elif action.get('host_probe') is not None:
                            if action['host_probe']==3:
                                assert super().request('Z0,a008ae58,2')=='OK'
                                assert super().request('Z0,a0089d02,2')=='OK'
                            changes={1:timer_va,2:temporary,10:native_runtime,11:action['host_probe'],32:int(manifest['screen_host_probe']['entry'],16)}
                        elif action.get('calendar') is not None:
                            output=temporary+64;action['calendar_output']=output
                            super().request(f'M{output:x},24:'+bytes(36).hex())
                            changes={1:timer_va,2:temporary,10:action['calendar'],11:output,32:int(manifest['native_abi']['calendar'],16)}
                        elif action.get('wall_time'):
                            changes={1:timer_va,2:temporary,10:0,32:int(manifest['native_abi']['wall_time'],16)}
                        elif action.get('native_page') is not None:
                            self.ui_state()
                            assert native_ui_root, 'native page requested before UI root exists'
                            changes={1:timer_va,2:temporary,10:native_ui_root,11:action['native_page'],12:action.get('direction',1),32:0xa00b3106}
                        elif 'framebuffer_ready' in action:
                            changes={1:timer_va,2:temporary,10:action['framebuffer_ready'],11:0,12:2,32:0xa00806d0}
                        else:
                            self.ui_state()
                            assert native_ui_root, 'native input requested before UI root exists'
                            changes={1:timer_va,2:temporary,10:native_ui_root,11:action['event'],12:param if action['event']==0x11 else 0,32:send_event_va}
                        updated=list(regs)
                        for register,value in changes.items():
                            super().request(f'P{register:x}={value.to_bytes(4,"little").hex()}');updated[register]=value
                        injected=(saved,action)
                        return struct.pack('<33I',*updated).hex()
            if pc in (0xa06810d8,0xa0681164,0xa06811e6):
                item={'pc':hex(pc),'a0':hex(regs[10]),'runtime':hex(regs[8])}
                if pc==0xa06810d8:item['mac']=super().request(f'm{regs[2]+16:x},6')
                debug_snapshots.append(item)
        return result
base.BufferedRSP=NativeRSP
if input_model:
    assert timer_va and send_event_va,'matching native ELF symbols required'
    profile['apis'][timer_va]='ui_input_gate'
class NativeProvider(OriginalProvider):
    def read(self,address,length):
        # QEMU's RSP PacketSize bounds the HEX response, not the byte count.
        return b''.join(super(NativeProvider,self).read(address+at,min(1024,length-at)) for at in range(0,length,1024))
    def write(self,address,data):
        for at in range(0,len(data),1024):super().write(address+at,data[at:at+1024])
    def __init__(self,rsp,port,ready,report):
        global native_provider
        super().__init__(rsp,port,ready,report)
        native_provider=self
        report['native_ports_bound']=[]
        threading.Thread(target=self.probe,daemon=True).start()
    def handle(self,name,args):
        fd,a1,a2,a3,a4,a5=args
        if name=='wall_epoch':
            self.report['rtc_model']='native AP01 RTC civil seconds = UTC + 28800; payload time() must normalize to UTC'
            return model_epoch[0]+28800
        if name=='wifi_mac':
            self.write(fd,synthetic_mac)
            self.report['synthetic_wifi_mac']=synthetic_mac.hex().upper()
            self.report['wifi_mac_model']='explicit MAC getter API fixture; native hardware/network identity not verified'
            return 0
        if name=='socket' and (fd,a1,a2)==(2,2,0):
            return self.allocate(socket.socket(socket.AF_INET,socket.SOCK_DGRAM))
        if name=='bind':
            address=self.read(a1,16);guest=int.from_bytes(address[2:4],'big')
            if a2!=16 or address[:2]!=b'\x02\0' or address[4:8]!=bytes(4) or guest not in (18080,18081,8767):raise ValueError('native bind contract')
            host={18080:self.port,18081:self.port+1,8767:self.port+2}[guest]
            self.sockets[fd].bind(('127.0.0.1',host));self.report['native_ports_bound'].append(guest)
            self.report['host_bound']=True;return 0
        if name=='recvfrom':
            if a2>257 or a3!=0x40 or int.from_bytes(self.read(a5,4),'little')!=16:raise ValueError('native UDP bounds')
            data,source=self.sockets[fd].recvfrom(a2);self.write(a1,data)
            self.write(a4,struct.pack('<H',2)+struct.pack('>H',source[1])+socket.inet_aton(source[0])+bytes(8));self.write(a5,struct.pack('<I',16));return len(data)
        if name=='sendto' and a4:
            if a2>1024 or a3!=0x40 or a5!=16:raise ValueError('native UDP send bounds')
            address=self.read(a4,16);destination=(socket.inet_ntoa(address[4:8]),int.from_bytes(address[2:4],'big'))
            if destination[0]!='127.0.0.1':raise ValueError('non-loopback native UDP destination')
            return self.sockets[fd].sendto(self.read(a1,a2),destination)
        return super().handle(name,args)
    def http(self,port,method,path,body=b'',token='',frame=False,device_token=False):
        body=bytes(body)
        with socket.create_connection(('127.0.0.1',port),timeout=360 if path.startswith('/api/firmware/') else 180 if frame and path=='/api/bundle/commit' else 60 if frame else 10) as stream:
            kind='application/vnd.codex-status.frame' if frame else 'application/octet-stream' if path.startswith('/api/firmware/chunk?') else 'application/json'
            header=f'{method} {path} HTTP/1.1\r\nHost: ap01\r\nContent-Length: {len(body)}\r\nContent-Type: {kind}\r\n'
            if token:header+=f'Authorization: Bearer {token}\r\n'
            if device_token:header+=f'X-Device-Token: {token}\r\n'
            stream.sendall(header.encode()+b'\r\n'+body);reply=b''
            while True:
                chunk=stream.recv(4096)
                if not chunk:break
                reply+=chunk
            head,payload=reply.split(b'\r\n\r\n',1)
            return int(head.split(b' ')[1]),payload if frame else json.loads(payload)
    def action(self,**values):
        action={**values,'done':threading.Event()};input_actions.put(action)
        if not action['done'].wait(90 if values.get('rebuild_original') else 20):raise TimeoutError('native LVGL input API')
        return action.get('value')
    def input(self,event,key=0,**values):
        result=self.action(event=event,key=key,**values)
        if event==0x11 and not values.get('sample'):
            until=time.monotonic()+30
            while time.monotonic()<until:
                state=self.action(observe=True)
                if state['root_ready'] and not state.get('host_phase'):break
                time.sleep(.2)
            else:raise TimeoutError('navigation slide did not finish')
        return result
    def snapshot(self,name):
        path=Path(sys.argv[sys.argv.index('--output')+1]).with_suffix('.'+name+'.ppm')
        action={'snapshot':str(path.resolve()),'done':threading.Event()};input_actions.put(action)
        if not action['done'].wait(30):raise TimeoutError('native screen capture')
        self.report.setdefault('native_screenshots',[]).append(str(path))
    def nor_snapshot(self,name):
        path=Path(sys.argv[sys.argv.index('--output')+1]).with_suffix('.'+name+'.flash.bin').resolve()
        action={'nor_snapshot':str(path),'done':threading.Event()};input_actions.put(action)
        if not action['done'].wait(30):raise TimeoutError('native NOR capture')
        return path
    def screen_host_transition_probe(self,count):
        self.action(onboarded=True);self.action(native_page=5,direction=0)
        self.input(0x11,0x13);state=self.action(observe=True)
        assert state['page']==7 and not state['focused']
        pages=[]
        for _ in range(count):
            self.input(0x11,0x13);state=self.action(observe=True)
            assert state['page']>=9 and state['focused'];pages.append(state['page'])
        assert len(set(pages))==count
        self.input(0x11,0x13);state=self.action(observe=True)
        assert state['page']==6 and not state['focused']
        for expected in reversed(pages):
            self.input(0x11,0x14);state=self.action(observe=True)
            assert state['page']==expected and state['focused']
        self.input(0x11,0x14);state=self.action(observe=True)
        assert state['page']==7 and not state['focused']
        self.input(0x11,0x13)
        self.report['native_screen_host_navigation']={'passed':True,'distinct_roots':pages,'pet':7,'settings':6}
        self.report['native_transition_passed']=True
    def transition_probe(self,count):
        def settled():
            until=time.monotonic()+30
            while time.monotonic()<until:
                state=self.action(observe=True)
                if state['root_ready']:return state
                time.sleep(.2)
            raise TimeoutError('native slide did not complete')
        # Complete the current offline virtual group before enabling the
        # explicit onboarding fixture. Native Wi-Fi/account/page filters stay
        # untouched; all transitions below use the real stock event handler.
        for _ in range(count):
            if not settled()['focused']:break
            self.input(0x11,0x13)
        assert not settled()['focused']
        self.action(onboarded=True)
        self.action(native_page=5,direction=0);settled()
        trace=[]
        self.report['native_pet_edge_trace']=trace
        def turn(key,page,focused,label):
            self.input(0x11,key)
            state=settled();trace.append(dict(label=label,key=key,**state))
            assert state['page']==page and bool(state['focused'])==focused,label
            return state
        # Date/Weather/Pet IDs are not a monotonic ring. Pet7->Settings6
        # used to insert another app, as did Settings6->Power0 on hardware.
        turn(0x13,7,True,'weather-to-first-template')
        for i in range(count-1):turn(0x13,7,True,'next-template')
        exits=[]
        self.report['native_same_window_exit']=exits
        def leave(key,page,label):
            state=self.input(0x11,key,sample=True)
            exits.append(dict(label=label,exit=state))
            assert not state['focused'] and state['page']==page and not state['root_ready'],label+' skipped animation'
            assert not state['content_hidden'] and not state['image_hidden'],label+' hid outgoing app'
            burst=self.input(0x11,key,sample=True)
            assert not burst['focused'] and burst['page']==page and not burst['root_ready'],label+' busy key changed navigation'
            exits[-1]['busy']=burst
            end=settled();exits[-1]['settled']=end
            assert end['page']==page and not end['focused'],label+' reentered app'
        leave(0x13,7,'forward-exit-to-pet')
        turn(0x13,6,False,'pet-to-settings')
        # Offline account fixture selects MiHome8 after Settings, then its
        # onboarding gate blocks rotation. Prepare Settings with the original
        # page API to test the reverse stock edge independently of that gate.
        turn(0x13,8,False,'settings-to-next-stock-page')
        self.action(native_page=6,direction=0);settled()
        turn(0x14,7,False,'settings-to-pet')
        # Reverse Pet follows the original availability filter (Weather5
        # when paired, Power0 in this offline fixture).
        self.input(0x11,0x14);entry=settled()
        trace.append(dict(label='pet-to-last-template',key=0x14,**entry))
        assert entry['focused'] and entry['page']!=7
        previous=entry['page']
        for i in range(count-1):turn(0x14,previous,True,'previous-template')
        leave(0x14,previous,'reverse-exit-to-stock')
        # The reverse stock neighbour is MiHome8 when offline; its onboarding
        # gate deliberately blocks further input. Prepare Weather as above,
        # then re-enter via the original event rather than bypassing that gate.
        self.action(native_page=5,direction=0);settled()
        turn(0x13,7,True,'return-through-weather-pet-edge')
        self.report['native_transition_passed']=True
    def slot_probe(self,token,identity):
        def call(method,path,body=b'',expected=200,credential=token):
            for attempt in range(5):
                code,value=self.http(self.port,method,path,body,credential)
                if code==503 and value.get('error')=='maintenance_busy':time.sleep(.05);continue
                if code!=expected:raise RuntimeError('slot response: '+str(code)+' '+str(value.get('error')))
                return value
            raise RuntimeError('slot bounded retry exhausted')
        slots=call('GET','/api/firmware/slots');status=call('GET','/api/firmware/status')
        assert slots['running_slot']==1 and slots['stable_slot']==(None if unconfigured_slots else 0)
        image=Path(sys.argv[sys.argv.index('--image')+1]).read_bytes()
        protected={'device_mac':identity,'request_id':'protected-slot','expected_revision':status['revision'],
                   'length':len(image),'recovery_crc':f'{int.from_bytes(image[-4:],"little"):08x}'}
        if not unconfigured_slots:
            reply=call('POST','/api/firmware/begin',json.dumps(protected).encode(),409)
            assert reply['error']=='stable_slot_protected'
        request={'device_mac':identity,'request_id':'switch-'+Path(sys.argv[sys.argv.index('--output')+1]).stem,
                 'target_slot':0,'expected_revision':status['revision']}
        if not unconfigured_slots:
            request.update(expected_stable_revision=slots['stable_revision'],expected_image_crc=slots['stable_image_crc'])
        body=json.dumps(request).encode();call('POST','/api/firmware/switch',body,401,'!'*32)
        stale={**request,'expected_image_crc':request.get('expected_image_crc',0)^1}
        call('POST','/api/firmware/switch',json.dumps(stale).encode(),409)
        before=self.nor_snapshot('before-slot-switch')
        calls=len(self.report['native_ff13_calls'])
        reply=call('POST','/api/firmware/switch',body)
        assert reply['result']=='applied' and reply['operation']=='switch' and reply['target_slot']==0 and reply['reboot_required']
        call('POST','/api/firmware/switch',body)
        assert len(self.report['native_ff13_calls'])==calls+1
        after=self.nor_snapshot('after-slot-switch')
        old=before.read_bytes();new=after.read_bytes()
        for start,end in ((0,0xe000),(0x10000,0xeb0000),(0xff0000,0x1000000)):
            assert old[start:end]==new[start:end],'slot switch changed a firmware/protected byte'
        self.report['native_slot_transaction']={'request_id':request['request_id'],'image_sha256':manifest['image']['sha256'],
            'target_slot':0,'firmware_slots_unchanged':True,'protected_regions_unchanged':True,
            'single_commit_after_replay':True,'before':str(before),'after':str(after)}
        self.report['native_slot_switch_passed']=True
    def firmware_probe(self,token,identity,frame):
        self.report['native_application_stage']='firmware-begin'
        image=Path(sys.argv[sys.argv.index('--image')+1]).read_bytes()
        request_id='firmware-'+Path(sys.argv[sys.argv.index('--output')+1]).stem
        def call(method,path,body=b'',expected=200,credential=token):
            for attempt in range(5):
                try:code,value=self.http(self.port,method,path,body,credential)
                except (OSError,ValueError):
                    if attempt==4:raise
                    self.report['native_firmware_original_queries']=self.report.get('native_firmware_original_queries',0)+1
                    time.sleep(.1);continue # Same identity/transaction/offset, never a new upload.
                if code==503 and value.get('result')=='rejected' and value.get('error')=='maintenance_busy':time.sleep(.05);continue
                if code!=expected:raise RuntimeError('firmware response: '+str(code)+' '+str(value.get('error')))
                return value
            raise RuntimeError('firmware bounded retry exhausted')
        slots=call('GET','/api/firmware/slots')
        if slots.get('stable_slot') is None and not unconfigured_slots:
            stable={'device_mac':identity,'request_id':'stable-'+request_id,'slot':slots['running_slot'],
                    'expected_stable_revision':slots['stable_revision'],'confirm':True}
            call('POST','/api/firmware/stable',json.dumps(stable).encode())
        status=call('GET','/api/firmware/status')
        envelope={'device_mac':identity,'request_id':request_id,'expected_revision':status['revision'],
                  'length':len(image),'recovery_crc':f'{int.from_bytes(image[-4:],"little"):08x}'}
        raw=json.dumps(envelope,separators=(',',':')).encode()
        call('POST','/api/firmware/begin',raw,401,'!'*32)
        before=self.nor_snapshot('before-ota')
        result=call('POST','/api/firmware/begin',raw)
        if result['result']!='receiving':raise RuntimeError('firmware did not begin')
        call('POST','/api/firmware/begin',raw) # Must not reopen/erase.
        health=call('GET','/health')
        if health['business_online']:raise RuntimeError('firmware did not gate business')
        rejected=frame(1,'/api/status',{},expected_status=503)
        if rejected.get('result')!='rejected':raise RuntimeError('business admission was not definite')
        chunk_size=result['max_chunk_bytes'];assert chunk_size==32768
        for at in range(0,len(image),chunk_size):
            path=f'/api/firmware/chunk?request_id={request_id}&offset={at}'
            block=image[at:at+chunk_size]
            reply=call('POST',path,block)
            if reply['received']!=at+len(block):raise RuntimeError('firmware byte correlation')
            if at==0:
                call('POST',path,block)
                altered=bytearray(block);altered[100]^=1;call('POST',path,altered,409)
            if at//chunk_size%16==0:call('GET','/health')
        self.report['native_application_stage']='firmware-commit'
        commit=json.dumps({'device_mac':identity,'request_id':request_id},separators=(',',':')).encode()
        result=call('POST','/api/firmware/commit',commit)
        if result['result']!='applied' or not result['reboot_required'] or result['boot_confirmed']:raise RuntimeError('firmware commit state')
        call('POST','/api/firmware/commit',commit)
        query=call('GET','/api/firmware/'+request_id)
        if query['result']!='applied':raise RuntimeError('firmware original query failed')
        after=self.nor_snapshot('after-ota')
        a=before.read_bytes();b=after.read_bytes()
        active=int(sys.argv[sys.argv.index('--active-slot')+1]) if '--active-slot' in sys.argv else 0
        target=0x7b0000 if active==0 else 0x10000
        assert len(a)==len(b)==0x1000000 and b[target:target+len(image)]==image
        # PT copies and /resource journal are the only other allowed changes.
        protected=((0,0xe000),(0x10000,0x7b0000),(0xff0000,0x1000000)) if active==0 else ((0,0xe000),(0x710000,0xeb0000),(0xff0000,0x1000000))
        for start,end in protected:
            if a[start:end]!=b[start:end]:raise RuntimeError('firmware changed protected NOR '+hex(start))
        self.report['native_firmware_transaction']={'request_id':request_id,'image_sha256':hashlib.sha256(image).hexdigest(),
            'bytes':len(image),'target_offset':hex(target),'before':str(before),'after':str(after),
            'full_inactive_image_matches':True,'protected_regions_unchanged':True,'reboot_required':True}
        self.report['native_firmware_protocol_passed']=True
    def pair_binding(self,info):
        token=secrets.token_hex(16);identity=info['device_mac'];principal='native-qemu-bridge';pair_id='native-pair'
        if resume_report:
            previous=json.loads(resume_report.read_bytes())
            assert previous['synthetic_wifi_mac']==identity==synthetic_mac.hex().upper()
            nor=Path(previous['flash_snapshot']['path']);data=nor.read_bytes()
            assert hashlib.sha256(data).hexdigest()==previous['flash_snapshot']['sha256']
            import zlib
            candidates=set();at=0
            while True:
                at=data.find(b'APB1',at)
                if at<0:break
                length=int.from_bytes(data[at+4:at+8],'little');record=data[at:at+length];at+=4
                if len(record)<16 or len(record)>2136 or record[8]!=1 or not 1<=record[9]<=8:continue
                if zlib.crc32(record[16:])!=int.from_bytes(record[12:16],'little'):continue
                position=16
                for _ in range(record[9]):
                    fields=[]
                    for _ in range(5):
                        n=record[position];position+=1;fields.append(record[position:position+n].decode());position+=n
                    if fields[0]==principal and fields[4]==pair_id:candidates.add(fields[3])
            assert len(candidates)==1,'one controlled synthetic binding is required'
            token=candidates.pop()
            code,reply=self.http(self.port,'GET','/health',token=token)
            assert code==200 and reply['device_mac']==identity
            self.report['native_binding_recovered']=True
            return token,identity,principal
        self.report['native_application_stage']='pairing'
        request={'device_mac':identity,'bridge_id':principal,'name':'Native ROM test','display_code':'1234',
                 'binding_token':token,'request_id':pair_id,'boot_id':info['boot_id'],'pairing_revision':info['pairing_revision']}
        code,reply=self.http(self.port,'POST','/api/pairing',json.dumps(request).encode())
        if code!=202:raise RuntimeError('native pairing begin rejected')
        until=time.monotonic()+30
        while time.monotonic()<until:
            code,reply=self.http(self.port,'GET','/api/pairing/'+pair_id,token=token)
            self.report['native_pairing_state']=reply.get('state')
            if reply.get('state')=='awaiting_confirmation':break
            time.sleep(.1)
        else:raise TimeoutError('native pairing dialog was not drawn')
        self.snapshot('pairing');self.input(0xb) # Old release cannot authorize.
        _,reply=self.http(self.port,'GET','/api/pairing/'+pair_id,token=token)
        if reply.get('state')=='approved':raise RuntimeError('stale native release approved pairing')
        self.input(1);self.input(0xb)
        code,reply=self.http(self.port,'GET','/api/pairing/'+pair_id,token=token)
        self.report['native_pairing_confirmation']={key:reply[key] for key in ('state','error','stage') if key in reply}
        self.report['native_pairing_confirmation']['http_status']=code
        if reply.get('state')!='approved':raise RuntimeError('native pairing confirmation failed')
        self.report['native_pairing_approved']=True
        return token,identity,principal
    def lifecycle_probe(self,frame,bundle,status,token,identity,principal,nonce):
        import copy,zlib
        self.report['native_application_stage']='coexistence'
        def probe(operation):return self.action(host_probe=operation)
        def idle():
            until=time.monotonic()+45
            while time.monotonic()<until:
                state=self.action(observe=True)
                if state['root_ready'] and not state.get('host_phase'):return state
                time.sleep(.1)
            raise TimeoutError('lifecycle slide/prepare')
        def drawn():
            until=time.monotonic()+45
            while time.monotonic()<until:
                state=frame(1,'/api/status',{})
                if state['display_state']=='displayed':return state
                time.sleep(.1)
            raise TimeoutError('rebuilt current template did not draw')
        def peer_drawn():
            until=time.monotonic()+45
            while time.monotonic()<until:
                if probe(10)>0:return
                time.sleep(.1)
            self.report['secondary_draw_diagnostic']={str(i):probe(i) for i in range(40,50)}
            self.report['secondary_draw_diagnostic']['ui']=self.action(observe=True)
            self.snapshot('peer-undrawn')
            raise TimeoutError('secondary application did not receive a real draw')
        def peer_pixels(scene,color):
            self.snapshot(scene)
            prefix=Path(sys.argv[sys.argv.index('--output')+1]).with_suffix('.'+scene)
            expected=struct.pack('<H',color)*76800
            diffs=[sum(a!=b for a,b in zip(Path(str(prefix)+f'.fb{i}.rgb565').read_bytes(),expected)) for i in (0,1)]
            assert min(diffs)==0,(scene,diffs)
            return {'scene':scene,'rgb565':color,'different_bytes':diffs}
        runtime=probe(0);assert runtime==native_runtime
        timer=probe(20);assert timer
        assert (probe(17),probe(18))==(17,18)
        threads=len(self.report['thread_create']);assert threads==3
        polls=probe(9)
        ids=status['template_ids']
        if not host_rebuild_only:
            assert len(ids)==8
            # Visit all eight actual codex roots, then cross the application boundary.
            for expected in ids[1:]:
                self.input(0x11,0x13);state=frame(1,'/api/status',{})
                assert state['active_template_id']==expected
            self.input(0x11,0x13);peer=self.action(observe=True)
            assert peer['page']==17 and not peer['focused']
            assert probe(9)>polls
            self.snapshot('peer-before-draw-ack')
            peer_drawn()
            pixels=[peer_pixels('peer-a',0xf800)]
            assert probe(26)==1
            for _ in range(3):self.action(observe=True)
            assert probe(28)==1 and probe(1)==0xfffffffe
            assert probe(27)==1
            for _ in range(3):self.action(observe=True)
            assert probe(28)==0
            self.input(0x11,10);assert probe(11)==1
            self.input(0x11,0x13);assert self.action(observe=True)['page']==17
            # Publish to codex while the other app has focus locked.
            replacement=copy.deepcopy(bundle);replacement['job_id']='peer-publish'
            replacement['profile']={'template_ids':[ids[0]],'initial_active_id':ids[0]}
            replacement['templates']=[next(t for t in bundle['templates'] if t['key']['template_id']==ids[0])]
            raw=json.dumps(replacement,ensure_ascii=False,separators=(',',':')).encode()
            request={'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,'request_id':'peer-publish',
                     'length':len(raw),'content_crc':f'{zlib.crc32(raw):08x}'}
            frame(10,'/api/bundle/begin',request)
            for at in range(0,len(raw),3072):frame(11,'/api/bundle/chunk?request_id=peer-publish&session_nonce='+nonce+'&offset='+str(at),raw[at:at+3072])
            ack=frame(12,'/api/bundle/commit',request)
            assert ack['result']=='applied'
            for _ in range(3):
                state=frame(1,'/api/status',{});ui=self.action(observe=True)
                assert ui['page']==17 and not ui['focused'] and probe(11)==1
                assert state['display_state']=='pending' and state['display_reason']=='waiting_for_template_page'
            pixels.append(peer_pixels('peer-bundle-unchanged',0xf800))
            # Real guest clock and button events: no synthetic timeout completion.
            self.input(1);until=time.monotonic()+60
            while probe(12)==0 and time.monotonic()<until:time.sleep(.1)
            assert probe(12)==1 and probe(11)==0
            self.input(0xb);self.input(0x11,10);assert probe(11)==0
            assert probe(14)==0 # logical B,A; physical root identity is unchanged.
            assert (probe(17),probe(18))==(17,18)
            self.input(0x11,0x14);assert self.action(observe=True)['page']==18
            pixels.append(peer_pixels('peer-b',0x07e0))
            self.input(0x11,0x14);assert self.action(observe=True)['focused']
            status=drawn();assert status['active_template_id']==ids[0]
            own=self.action(observe=True)['page'];assert probe(16)==0
            assert self.action(observe=True)['page']==own and probe(15)==0
            self.report['native_coexistence']={'passed':True,'codex_templates_visited':ids,'peer_roots':[17,18],
                'peer_hidden_poll':True,'system_modal_hides_peer_and_blocks_switch':True,'independent_draws_and_pixels':pixels,'foreign_bundle_preserves_focus_lock':True,
                'real_3s_escape':True,'stable_peer_reorder':True,'peer_removal_preserves_codex_focus':True}
        self.report['native_application_stage']='ui-rebuild'
        cycles=[]
        self.report['native_ui_rebuild']={'passed':False,'cycles':cycles}
        for cycle in range(2):
            before=self.action(observe=True);context=status['active_context_id'];epoch=probe(21)
            revision=probe(23);assert probe(6)==1
            deleting_phase=0;deleting_x=0
            assert probe(50)==4
            if cycle==1:
                assert probe(52)==0
                until=time.monotonic()+30
                while probe(22)!=2 and time.monotonic()<until:time.sleep(.1)
                deleting_phase=probe(53);deleting_x=probe(54)
                if deleting_x>=0x80000000:deleting_x-=0x100000000
                assert probe(22)==2 and deleting_phase==2 and 0<abs(deleting_x)<320, (deleting_phase,deleting_x)
            else:assert probe(3)==2 and probe(22)==2
            assert probe(51)==0
            assert self.action(observe=True)['page']==-1
            code,health=self.http(self.port,'GET','/health',token=token);assert code==200
            request=json.dumps({'magic':'codex-status','kind':'discover','discovery_v':1,'nonce':'0123456789abcdef0123456789abcdef'}).encode()
            with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as udp:
                udp.bind(('127.0.0.1',0));udp.settimeout(10);udp.sendto(request,('127.0.0.1',self.port+2));reply,_=udp.recvfrom(1024)
            assert json.loads(reply)['mac']==identity
            assert probe(0)==runtime and probe(20)==timer and self.action(rebuild_original=True)
            until=time.monotonic()+45
            while time.monotonic()<until:
                result=probe(5)
                if result==0:break
                time.sleep(.1)
            else:raise TimeoutError('rebuilt screen set not published')
            after=idle();status=drawn()
            assert after['focused'] and status['active_context_id']==context
            assert probe(21)==epoch+1 and probe(24)>revision and probe(25)==0xfffffffd
            assert probe(0)==runtime and probe(20)==timer and probe(50)==4
            assert probe(7)==1 # Present an old-epoch frame after reattachment.
            for _ in range(3):
                self.action(observe=True);state=frame(1,'/api/status',{})
                assert self.action(observe=True)['generation']!=0xdeadbeef
                assert state['active_context_id']==context
            self.snapshot('rebuilt-'+str(cycle))
            assert probe(8)==1 and probe(8)==1 # repeated hook calls create no new executor/timer.
            assert len(self.report['thread_create'])==threads
            assert probe(1)==0;assert idle()['page']==17
            peer_drawn()
            assert probe(5)==0;idle();status=drawn()
            cycles.append({'cycle':cycle,'epoch':epoch+1,'deleted_host_phase':deleting_phase,'target_x_at_delete':deleting_x,'stock_bindings_retired_and_rebound':True,'runtime_reused':True,'timer_reused':True,
                'thread_count':threads,'maintenance_and_discovery_while_detached':True,'old_revision_rejected':True,
                'old_frame_rejected':True,'new_current_template_drawn':True,'peer_rebound':True})
        self.report['native_ui_rebuild']={'passed':True,'cycles':cycles,'stock_page_bodies':'all nine created by replaying original theme constructor; lv_init/fb/input/main loop not replayed'}
    def application_probe(self,info):
        token,identity,principal=self.pair_binding(info)
        if model_epoch:
            utc=self.action(wall_time=True)
            assert utc==model_epoch[0],('RTC normalization',utc,model_epoch[0])
            self.report['native_rtc_normalization']={'raw_civil_epoch':model_epoch[0]+28800,'utc_epoch':utc,'passed':True}
            cases=[(1792429140,[2026,10,20,0,59]),(1798732800,[2027,1,1,0,0]),(1709224200,[2024,3,1,0,30])]
            verified=[]
            for epoch,expected in cases:
                fields=self.action(calendar=epoch)
                actual=[fields[5]+1900,fields[4]+1,fields[3],fields[2],fields[1]]
                assert actual==expected,('native local calendar',actual,expected)
                verified.append({'epoch':epoch,'local':actual})
            self.report['native_local_calendar']=verified
        if not fallback_receipt:
            until=time.monotonic()+120
            while True:
                code,safety=self.http(self.port,'GET','/api/firmware/slots',token=token)
                if code!=200:raise RuntimeError('boot safety status rejected')
                if safety.get('boot_safety')!='armed':break
                if time.monotonic()>=until:raise TimeoutError('boot health window')
                time.sleep(.1)
            self.report['native_boot_safety']=safety.get('boot_safety')
            if self.report.get('native_boot_prearmed'):
                assert safety['boot_safety']=='healthy_locked'
                path=self.nor_snapshot('healthy-locked');tables=partition_tables(path.read_bytes())
                armed=self.report['native_boot_prearmed'];assert tables[-1]['slot']==1 and tables[-1]['age']==armed['tables'][-1]['age']+1
                assert len(tables)==2 and tables[0]['slot']==0 and tables[0]['age']==armed['tables'][-1]['age']
                before=Path(armed['path']).read_bytes();after=path.read_bytes()
                for start,end in ((0,0xe000),(0x10000,0xeb0000),(0xff0000,0x1000000)):assert before[start:end]==after[start:end]
                calls=self.report['native_ff13_calls'];assert len(calls)==2 and all(call['result']==0 for call in calls)
                elapsed=calls[1]['guest_uptime_ms']-calls[0]['guest_uptime_ms'];assert 9000<=elapsed<30000
                self.report['native_boot_health_verified']={'path':str(path),'tables':tables,'firmware_slots_unchanged':True,'elapsed_ms':elapsed}
        session=secrets.token_bytes(16);number=0;nonce=''
        def frame(op,path,payload,claim=False,expected_status=200):
            nonlocal session,number
            if not isinstance(payload,bytes):
                payload={k:v for k,v in payload.items() if k not in ('op','session_nonce')}
            body=payload if isinstance(payload,bytes) else json.dumps(payload,separators=(',',':')).encode()
            for attempt in range(5):
                number+=1
                raw=struct.pack('>BBHHH',0xc2,0,op,0,len(body))+session+struct.pack('>Q',number)+bytes(16)+body
                for transport_attempt in range(3):
                    try:
                        code,reply=self.http(self.port+1,'POST',path,raw,token,True,claim);break
                    except (OSError,ValueError):
                        if transport_attempt==2:raise
                        self.report['native_transport_original_queries']=self.report.get('native_transport_original_queries',0)+1
                        time.sleep(.1) # Query the SAME wire request/session/number.
                if reply[:1]==b'{':value=json.loads(reply)
                else:
                    if len(reply)<48 or reply[0]!=0xc2 or int.from_bytes(reply[24:32],'big')!=number:raise RuntimeError('native CompactFrame correlation')
                    session=reply[8:24];value=json.loads(reply[48:])
                if code==503 and value.get('error')=='bindings_busy' and value.get('result')=='rejected':
                    self.report['native_binding_admission_retries']=self.report.get('native_binding_admission_retries',0)+1
                    time.sleep(.05);continue # Explicit rejection before dispatch; same application transaction.
                if op==1 and value.get('error')=='retired':continue # Read-only status had no retained large reply.
                if code!=expected_status:raise RuntimeError('native operation rejected: '+str(value.get('error',code)))
                return value
            raise RuntimeError('native binding admission retry budget exhausted')
        status=frame(1,'/api/status',{});nonce=status['session_nonce']
        frame(4,'/claim',{'bridge_id':principal,'device_mac':identity},True)
        fixture=Path(json.loads(resume_report.read_bytes()).get('native_bundle_fixture',str(native_bundle))) if resume_report else native_bundle
        fixture_bytes=fixture.read_bytes();bundle=json.loads(fixture_bytes)
        self.report['native_bundle_fixture']=str(fixture.resolve())
        self.report['native_bundle_fixture_sha256']=hashlib.sha256(fixture_bytes).hexdigest()
        bundle['bridge_id']=principal;bundle['device_mac']=identity
        raw=json.dumps(bundle,ensure_ascii=False,separators=(',',':')).encode()
        import zlib
        envelope={'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,'request_id':'native-bundle',
                  'length':len(raw),'content_crc':f'{zlib.crc32(raw):08x}'}
        if resume_report:
            previous=json.loads(resume_report.read_bytes())
            if previous.get('native_bundle_fixture_sha256'):
                assert hashlib.sha256(fixture_bytes).hexdigest()==previous['native_bundle_fixture_sha256']
            prior=previous.get('native_persisted_status',previous['native_scenes'][-1])
            assert status['configured'] and status['active_template_id']==prior['active_template_id']
            assert status['active_context_id']!=prior['active_context_id'] and status['applied_seq']==0
            recovered=status['active_context_id'];frame(12,'/api/bundle/commit',envelope)
            if previous.get('native_activation_request'):
                activation={**previous['native_activation_request'],'session_nonce':nonce}
                frame(13,'/api/activate',activation)
                self.report['native_activation_recovered']=True
            status=frame(1,'/api/status',{})
            assert status['active_context_id']==recovered,'durable Bundle replay changed context'
            self.report['native_recovered_status']={key:status.get(key) for key in ('active_context_id','active_template_id','applied_seq','template_ids')}
            self.report['native_bundle_recovered']=True
        else:
            self.report['native_application_stage']='bundle'
            frame(10,'/api/bundle/begin',envelope)
            for at in range(0,len(raw),3072):
                path=f'/api/bundle/chunk?request_id=native-bundle&session_nonce={nonce}&offset={at}'
                frame(11,path,raw[at:at+3072])
            self.report['native_application_stage']='commit'
            frame(12,'/api/bundle/commit',envelope)
            self.report['native_bundle_committed']=True
        status=frame(1,'/api/status',{})
        if firmware_receipt:
            previous=json.loads(firmware_receipt.read_bytes())
            transaction=previous.get('native_slot_transaction') or previous['native_firmware_transaction']
            if not fallback_receipt:assert transaction['image_sha256']==manifest['image']['sha256']
            code,receipt=self.http(self.port,'GET','/api/firmware/'+transaction['request_id'],token=token)
            if fallback_receipt:
                assert code==200 and receipt.get('result')=='rejected' and receipt.get('error')=='boot_trial_pending'
                self.report['native_boot_fallback_receipt']=True
            elif code!=200 or receipt.get('result')!='applied' or not receipt.get('boot_confirmed') or receipt.get('reboot_required'):
                raise RuntimeError('firmware durable boot receipt not confirmed')
            code,health=self.http(self.port,'GET','/health',token=token)
            if code!=200 or not health['business_online']:raise RuntimeError('business did not recover after firmware boot')
            self.report['native_firmware_receipt_recovered']=True
            if 'native_slot_transaction' in previous:self.report['native_slot_receipt_recovered']=True
        if firmware_only:
            self.firmware_probe(token,identity,frame);self.report['native_application_stage']='complete';return
        # A recovered native page need not be the last stock page. Navigate
        # through actual input events, waiting for each native slide, rather
        # than assuming that one key always enters the virtual group.
        if manifest.get('screen_host'):
            self.action(onboarded=True);self.action(native_page=7,direction=0)
        for _ in range(24):
            state=self.action(observe=True)
            if state['focused']:break
            if state['root_ready']:self.input(0x11,0x13)
        else:raise TimeoutError('native virtual group entry budget exhausted')
        status=frame(1,'/api/status',{})
        self.report['native_application_stage']='data'
        values={'buckets[codex].5h.remaining':86,'buckets[codex].weekly.remaining':72,
                'buckets[codex].weekly.resetsAt':1792749600,'buckets[codex].5h.resetsAt':1792308600,
                'account.plan':'pro','bridge.label':'username','resetCredits.availableCount':2}
        fields=[{'i':r['index'],'k':r['field'],'v':values.get(r['field']),'q':'fresh' if r['field'] in values else 'missing'}
                for r in status['data_contract']['requirements'] if not r['local']]
        fingerprint=''.join(f'{f["i"]}:{f["k"]}:'+('~' if f['v'] is None else json.dumps(f['v'],ensure_ascii=False,separators=(',',':')))+f':{f["q"]};' for f in fields).encode()
        frame(2,'/api/data',{'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,
            'request_id':'native-data-1','active_context_id':status['active_context_id'],'seq':1,'crc':f'{zlib.crc32(fingerprint):08x}','fields':fields})
        self.report['native_data_applied']=True
        context=status['active_context_id']
        until=time.monotonic()+30
        while time.monotonic()<until:
            status=frame(1,'/api/status',{})
            if status['active_context_id']!=context:raise RuntimeError('entry unexpectedly changed the active context')
            if status['display_state']=='displayed' and status['applied_seq']==1:break
            time.sleep(.1)
        else:raise TimeoutError('native data frame was not drawn')
        self.snapshot('codex')
        self.report['native_data_status']={key:status.get(key) for key in ('active_template_id','active_context_id','applied_seq','frame_generation','frame_crc','display_state')}
        self.report['native_codex_display_ack']=True
        self.report['native_scenes']=[{'scene':'codex',**self.report['native_data_status']}]
        self.report['native_storage']={key:status.get(key) for key in ('free_bytes','max_bundle_bytes','max_bundle_decoded_bytes','bundle_compression')}
        def show_scene(name,values,seq):
            nonlocal status
            fields=[{'i':r['index'],'k':r['field'],'v':values.get(r['field']),
                     'q':'fresh' if r['field'] in values else 'missing'}
                    for r in status['data_contract']['requirements'] if not r['local']]
            fingerprint=''.join(f'{f["i"]}:{f["k"]}:'+('~' if f['v'] is None else json.dumps(f['v'],ensure_ascii=False,separators=(',',':')))+f':{f["q"]};' for f in fields).encode()
            frame(2,'/api/data',{'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,
                  'request_id':f'native-data-{seq}','active_context_id':context,'seq':seq,'crc':f'{zlib.crc32(fingerprint):08x}','fields':fields})
            until=time.monotonic()+30
            while time.monotonic()<until:
                status=frame(1,'/api/status',{})
                if status['active_context_id']!=context:raise RuntimeError('scene context changed')
                if status['display_state']=='displayed' and status['applied_seq']==seq:break
                time.sleep(.1)
            else:raise TimeoutError('native scene was not drawn: '+name)
            self.snapshot(name)
            self.report['native_scenes'].append({'scene':name,**{key:status.get(key) for key in ('active_context_id','active_template_id','applied_seq','frame_generation','frame_crc','display_state')}})
        self.report['native_application_stage']='visual-cases'
        weekly={k:v for k,v in values.items() if '.5h.' not in k}
        show_scene('weekly-only',weekly,2)
        show_scene('zero',{**values,'buckets[codex].5h.remaining':0,'buckets[codex].weekly.remaining':0,'resetCredits.availableCount':0},3)
        show_scene('full',{**values,'buckets[codex].5h.remaining':100,'buckets[codex].weekly.remaining':100},4)
        if model_epoch:
            before=self.action(observe=True);before_status=status.copy();model_epoch[0]+=60
            until=time.monotonic()+40
            while time.monotonic()<until:
                observed=self.action(observe=True)
                if observed['generation']>before['generation']:break
            else:raise TimeoutError('clock did not refresh without network traffic')
            until=time.monotonic()+30
            while time.monotonic()<until:
                status=frame(1,'/api/status',{})
                if status['display_state']=='displayed':break
            else:raise TimeoutError('minute clock frame was not drawn')
            self.snapshot('clock-next-minute')
            assert status['applied_seq']==before_status['applied_seq'] and status['active_context_id']==before_status['active_context_id']
            assert status['frame_crc']!=before_status['frame_crc']
            self.report['native_clock_minute_refresh']={'passed':True,'data_sequence_unchanged':True,
                'context_unchanged':True,'business_requests_during_refresh_wait':0,'before':before,'after':observed}
        if host_lifecycle_probe:
            self.lifecycle_probe(frame,bundle,status,token,identity,principal,nonce)
            self.report['native_application_stage']='complete'
            return
        self.report['native_application_stage']='navigation'
        ids=status['template_ids'];navigation=[]
        if not screen_host_probe_only:
            for expected in ids[1:]:
                before=status['active_context_id'];self.input(0x11,0x13);status=frame(1,'/api/status',{})
                assert status['active_template_id']==expected and status['active_context_id']!=before and status['applied_seq']==0
                navigation.append(expected)
            last=status['active_context_id'];self.input(0x11,0x13);status=frame(1,'/api/status',{})
            assert status['active_context_id']==last # Exit to the original page.
            self.input(0x11,0x14);status=frame(1,'/api/status',{})
            assert status['active_template_id']==ids[-1] and status['active_context_id']==last
            for expected in reversed(ids[:-1]):
                self.input(0x11,0x14);status=frame(1,'/api/status',{})
                assert status['active_template_id']==expected and status['applied_seq']==0
                navigation.append(expected)
            self.input(0x11,0x14);self.input(0x11,0x13);status=frame(1,'/api/status',{})
            assert status['active_template_id']==ids[0]
            self.report['native_navigation']={'template_count':len(ids),'visited':navigation,'forward_reverse_and_original_return':True}
        if bundle_replace_probe:
            import copy
            self.report['native_application_stage']='bundle-page-restore'
            checks=[]
            def replace(order,initial,job):
                replacement=copy.deepcopy(bundle);replacement['job_id']=job
                replacement['profile']={'template_ids':order,'initial_active_id':initial}
                replacement['templates']=[next(t for t in bundle['templates'] if t['key']['template_id']==id) for id in order]
                raw=json.dumps(replacement,ensure_ascii=False,separators=(',',':')).encode()
                request={'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,
                    'request_id':job,'length':len(raw),'content_crc':f'{zlib.crc32(raw):08x}'}
                frame(10,'/api/bundle/begin',request)
                for at in range(0,len(raw),3072):
                    frame(11,f'/api/bundle/chunk?request_id={job}&session_nonce={nonce}&offset={at}',raw[at:at+3072])
                ack=frame(12,'/api/bundle/commit',request)
                assert ack['result']=='applied' and ack['display_state']=='pending'
                return ack
            def drawn(id):
                until=time.monotonic()+30
                while time.monotonic()<until:
                    state=frame(1,'/api/status',{})
                    assert state['active_template_id']==id
                    if state['display_state']=='displayed':return state
                    time.sleep(.1)
                raise TimeoutError('replacement template did not draw automatically')
            # Current template is first; move it to the last position, declaring
            # a different initial page. Identity, focus and real LVGL draw win.
            old_context=status['active_context_id']
            ack=replace(list(reversed(ids)),ids[-1],'replace-reorder')
            assert ack['active_template_id']==ids[0]
            status=drawn(ids[0]);assert status['active_context_id']!=old_context
            assert self.action(observe=True)['focused']
            checks.append('focused_reorder_preserves_id_and_draws')
            self.snapshot('replace-reorder')
            # Deleting the displayed page uses the new initial page and also
            # proves the 1-page group returns without a rotary event.
            replace([ids[-1]],ids[-1],'replace-remove')
            status=drawn(ids[-1]);assert self.action(observe=True)['focused']
            checks.append('removed_page_uses_initial_single_page_draws')
            replace([ids[-1]],ids[-1],'replace-single');status=drawn(ids[-1])
            checks.append('single_page_update_draws')
            self.input(0x11,0x13);before=self.action(observe=True)
            assert not before['focused']
            ack=replace(ids,ids[0],'replace-stock')
            for _ in range(3):
                time.sleep(.1);status=frame(1,'/api/status',{})
                state=self.action(observe=True)
                assert not state['focused'] and state['page']==before['page']
                assert status['display_state']=='pending' and status['display_reason']=='waiting_for_template_page'
            assert ack['display_reason']=='waiting_for_template_page'
            checks.append('stock_page_preserved_with_explicit_feedback')
            self.input(0x11,0x14);status=drawn(ids[-1])
            checks.append('reentry_confirms_only_current_template')
            for _ in ids[:-1]:self.input(0x11,0x14)
            status=drawn(ids[0]);assert status['template_ids']==ids
            self.report['native_bundle_replace']={'passed':True,'checks':checks,'status':{
                k:status[k] for k in ('active_template_id','display_state','display_reason','template_ids')}}
        if transition_probe:
            (self.screen_host_transition_probe if manifest.get('screen_host') else self.transition_probe)(len(ids));status=frame(1,'/api/status',{})
        activation={'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,
                    'request_id':'native-activate-'+Path(sys.argv[sys.argv.index('--output')+1]).stem,
                    'template_id':ids[1],'expected_active_context_id':status['active_context_id']}
        frame(13,'/api/activate',activation);status=frame(1,'/api/status',{})
        assert status['active_template_id']==ids[1] and status['active_context_id']!=activation['expected_active_context_id']
        activated=status['active_context_id'];frame(13,'/api/activate',activation);status=frame(1,'/api/status',{})
        assert status['active_context_id']==activated
        self.report['native_activation_request']={key:value for key,value in activation.items() if key!='session_nonce'}
        self.report['native_persisted_status']={key:status.get(key) for key in ('active_context_id','active_template_id','applied_seq','template_ids')}
        self.report['native_activation_replay_passed']=True
        if firmware_probe:self.firmware_probe(token,identity,frame)
        if slot_probe:self.slot_probe(token,identity)
        self.report['native_application_stage']='complete'
    def probe(self):
        deadline=time.monotonic()+100
        while time.monotonic()<deadline:
            try:
                if not all(port in self.report['native_ports_bound'] for port in (18080,18081,8767)):
                    time.sleep(.05);continue
                code,info=self.http(self.port,'GET','/api/pairing/info')
                self.report['native_pairing_info']={'status':code,'mac':info.get('device_mac'),'method':info.get('binding_method')}
                code,denied=self.http(self.port,'GET','/health');self.report['native_unauthorized_health']=code
                # Menu-code maintenance deliberately returns 503 until a code
                # exists. It must not mint a credential while serving HTTP.
                self.report['native_unauthorized_health_error']={key:denied.get(key) for key in ('error','stage','action')}
                request=json.dumps({'magic':'codex-status','kind':'discover','discovery_v':1,'nonce':'0123456789abcdef0123456789abcdef'}).encode()
                with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as udp:
                    udp.bind(('127.0.0.1',0));udp.settimeout(5);udp.sendto(request,('127.0.0.1',self.port+2));reply,_=udp.recvfrom(1024)
                    discovered=json.loads(reply)
                self.report['native_discovery_matches']=discovered['mac']==info['device_mac'] and discovered['http_port']==18081 and discovered['maintenance_port']==18080
                if input_model:
                    try:self.application_probe(info)
                    except Exception as error:
                        import traceback
                        self.report['native_application_error']=type(error).__name__+': '+str(error)
                        self.report['native_application_traceback']=traceback.format_exc()
                    self.report['native_probe_finished']=True
                return
            except (ConnectionError,OSError,ValueError) as error:
                self.report['native_probe_error']=type(error).__name__;time.sleep(.1)
base.Provider=NativeProvider
if __name__=='__main__':
    OriginalQMP=base.runner.QMP
    screenshot=Path(sys.argv[sys.argv.index('--output')+1]).with_suffix('.ppm')
    class CaptureQMP(OriginalQMP):
        def __init__(self,*args,**kwargs):
            global native_qmp
            super().__init__(*args,**kwargs);native_qmp=self
        def command(self,name,args=None):
            if name=='human-monitor-command' and not screenshot.exists():
                super().command('screendump',{'filename':str(screenshot.resolve())})
                if native_provider:
                    state={'main_input_group':native_provider.read(0x62fc95bc,4).hex(),
                        'native_init_flag':native_provider.read(0x62fc4038,1).hex(),
                        'screen_state':native_provider.read(0x62fc4039,1).hex(),
                        'lcd':super().command('qom-get',{'path':'/machine','property':'lcd-state'})}
                    if native_ui_root:state['root']=native_provider.read(native_ui_root,104).hex()
                    display=int.from_bytes(native_provider.read(0x62fc9590,4),'little')
                    if display:
                        display_bytes=native_provider.read(display,0x310)
                        state['display']=hex(display);state['display_bytes']=display_bytes.hex()
                        timer=int.from_bytes(display_bytes[0x308:0x30c],'little')
                        if timer:state['display_timer']=native_provider.read(timer,24).hex()
                        screen=int.from_bytes(display_bytes[0x2c0:0x2c4],'little')
                        if screen:state['active_screen']=native_provider.read(screen,44).hex()
                    if native_runtime and manifest.get('native_abi',{}).get('runtime_offsets'):
                        state['runtime']=hex(native_runtime);state['fields']={}
                        for field_name,offset in manifest['native_abi']['runtime_offsets'].items():
                            count=1 if field_name in ('pairPending','pairWoken','focused','retired','ready') else 4
                            value=int.from_bytes(native_provider.read(native_runtime+offset,count),'little');state['fields'][field_name]=value
                            if field_name in ('content','dialog','image') and value:
                                object_bytes=native_provider.read(value,44);state[field_name]=object_bytes.hex()
                                parent=int.from_bytes(object_bytes[4:8],'little')
                                if parent:state[field_name+'_parent']=native_provider.read(parent,44).hex()
                    screenshot.with_suffix('.ui-state.json').write_text(json.dumps(state,indent=2),encoding='utf-8')
                    for index in (0,1):
                        pixels=b''.join(native_provider.read(0xa8000020+153600*index+at,min(1024,153600-at)) for at in range(0,153600,1024))
                        screenshot.with_suffix(f'.fb{index}.rgb565').write_bytes(pixels)
            return super().command(name,args)
    base.runner.QMP=CaptureQMP
    result=base.main()
    path=Path(sys.argv[sys.argv.index('--output')+1]);report=json.loads(path.read_text())
    denial=report.get('native_unauthorized_health_error',{})
    denied=report.get('native_unauthorized_health')==401 or (report.get('native_unauthorized_health')==503 and
        denial=={'error':'device_code_unavailable','stage':'manual_auth','action':'open_device_code_menu'})
    report['native_service_probe_passed']=bool(report.get('native_pairing_info',{}).get('status')==200 and denied and report.get('native_discovery_matches'))
    report['cpu_safety_passed']=report['passed']
    # The frozen runner's UART marker may be interleaved by other task logs.
    # A matching-ELF UI timer breakpoint gives direct execution evidence even
    # when that literal log line is absent. Functional/pixel checks stay separate.
    report['native_ui_timer_observed']=bool(native_runtime and native_ui_root)
    report['native_execution_clean']=not report.get('error') and not report.get('first_guest_exception') and report.get('source_unchanged') and (report.get('native_ui_started') or report['native_ui_timer_observed'])
    report['native_application_smoke_passed']=bool((report.get('native_pairing_approved') or report.get('native_binding_recovered')) and
        (report.get('native_bundle_committed') or report.get('native_bundle_recovered')) and
        report.get('native_data_applied') and report.get('native_codex_display_ack') and len(report.get('native_scenes',[]))==4)
    functional=report.get('native_firmware_protocol_passed') if firmware_only else report['native_application_smoke_passed']
    report['passed']=bool(not report.get('native_application_error') and report['native_execution_clean'] and report['native_service_probe_passed'] and
                          (functional if input_model else report['cpu_safety_passed']) and (report.get('native_firmware_protocol_passed') if firmware_probe else True) and
                          (report.get('native_slot_switch_passed') if slot_probe else True) and
                          (report.get('native_transition_passed') if transition_probe else True))
    if bundle_replace_probe:report['passed']=bool(report['passed'] and report.get('native_bundle_replace',{}).get('passed'))
    if host_lifecycle_probe:report['passed']=bool(report['passed'] and (host_rebuild_only or report.get('native_coexistence',{}).get('passed')) and report.get('native_ui_rebuild',{}).get('passed') and report.get('native_application_stage')=='complete')
    if trial_cut:report['passed']=bool(report.get('native_boot_trial_cut') and report['native_execution_clean'] and len(report['native_ff13_calls'])==1)
    report['full_application_verified']=bool(report['native_application_smoke_passed'] and all(report.get(key) for key in
        ('native_binding_recovered','native_bundle_recovered','native_activation_recovered','native_firmware_receipt_recovered')))
    report['limitations']=['Socket provider/MAC/LVGL input/fbdev readiness are explicit API fixtures, not physical hardware',
                          'Actual original LVGL framebuffer is captured; LCD/SPI completion remains unverified',
                          'Full persistence/update/resource release acceptance is tracked separately']
    report['host_rebuild_only']=host_rebuild_only
    report['native_runner_sha256']=NATIVE_RUNNER_SHA
    report['runner_variant_sha256']=runner_variant_sha
    report['native_startup_snapshots']=debug_snapshots
    report['input_api_model']=input_model
    report['framebuffer_ready_api_model']=fb_ready_model
    report['native_input_events']=input_trace
    path.write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps({'native_services':report['native_service_probe_passed'],'full_application_verified':report['full_application_verified'],'report':str(path)}))
    sys.exit(0 if report['passed'] else 1)
