// Offline analysis of the immutable 0041 image with the frozen private model.
// @category AP01.Native
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import com.google.gson.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;

public class Ap01NativeExports extends GhidraScript {
    @Override public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 2) throw new IllegalArgumentException("output.json addresses...");
        MessageDigest sha = MessageDigest.getInstance("SHA-256");
        byte[] bytes = new byte[65536];
        Address base = toAddr("0x9ffff000");
        for (int pos = 0; pos < 6769704;) {
            int n = Math.min(bytes.length, 6769704 - pos);
            if (currentProgram.getMemory().getBytes(base.add(pos), bytes, 0, n) != n)
                throw new IllegalStateException("Incomplete image");
            sha.update(bytes, 0, n); pos += n;
        }
        String digest = java.util.HexFormat.of().formatHex(sha.digest());
        if (!digest.equals("972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9"))
            throw new IllegalStateException("Wrong0041 image");
        if (!currentProgram.getLanguageID().toString().equals("RISCV:LE:32:ap01thead2"))
            throw new IllegalStateException("Requires the frozen private THead model");
        DecompInterface decompiler = new DecompInterface();
        if (!decompiler.openProgram(currentProgram)) throw new IllegalStateException("Decompiler");
        JsonObject report = new JsonObject();
        report.addProperty("source_sha256", digest);
        report.addProperty("language", currentProgram.getLanguageID().toString());
        report.addProperty("scope", "offline recovered ABI candidates; no native execution proof");
        JsonArray functions = new JsonArray();
        for (int i = 1; i < args.length; ++i) {
            Address entry = toAddr(args[i]);
            Function f = currentProgram.getFunctionManager().getFunctionAt(entry);
            if (f == null) f = currentProgram.getFunctionManager().getFunctionContaining(entry);
            if (f == null) {
                new DisassembleCommand(entry, null, true).applyTo(currentProgram, monitor);
                new CreateFunctionCmd(entry).applyTo(currentProgram, monitor);
                f = currentProgram.getFunctionManager().getFunctionAt(entry);
            }
            JsonObject object = new JsonObject();
            object.addProperty("address", args[i]);
            if (f == null) { object.addProperty("error", "function_unresolved"); functions.add(object); continue; }
            object.addProperty("name", f.getName());
            object.addProperty("body_bytes", f.getBody().getNumAddresses());
            DecompileResults result = decompiler.decompileFunction(f, 90, monitor);
            if (result.decompileCompleted()) object.addProperty("pseudocode", result.getDecompiledFunction().getC());
            else object.addProperty("error", result.getErrorMessage());
            JsonArray assembly = new JsonArray();
            for (Instruction instruction : currentProgram.getListing().getInstructions(f.getBody(), true))
                assembly.add(instruction.getAddress() + ": " + instruction.toString());
            object.add("assembly", assembly);
            JsonArray callees = new JsonArray();
            for (Function callee : f.getCalledFunctions(monitor)) callees.add("0x" + callee.getEntryPoint().toString());
            object.add("callees", callees); functions.add(object);
            JsonArray callers = new JsonArray();
            for (Function caller : f.getCallingFunctions(monitor)) callers.add("0x" + caller.getEntryPoint().toString());
            object.add("callers", callers);
        }
        decompiler.dispose(); report.add("functions", functions);
        JsonArray literals = new JsonArray();
        byte[] image = new byte[6769704];
        currentProgram.getMemory().getBytes(base, image);
        for (String word : new String[]{"wlan0", "netlib_getmacaddr", "wapi_get_mac", "get_mac", "getmac", "lv_image_cache_drop", "lv_image_cache_resize", "lv_image_cache_is_enabled", "lv_event_get_user_data", "lv_obj_send_event", "lv_obj_event_send", "lv_event_send", "lv_event_get_key", "lv_obj_update_layout", "lv_obj_get_width", "lv_obj_refr_size"}) {
            byte[] needle = (word + "\0").getBytes(StandardCharsets.UTF_8);
            for (int pos = 0; pos + needle.length <= image.length; ++pos) {
                boolean match = true;
                for (int j = 0; j < needle.length; ++j) if (image[pos+j] != needle[j]) { match = false; break; }
                if (!match) continue;
                JsonObject literal = new JsonObject(); literal.addProperty("text", word);
                Address address = base.add(pos); literal.addProperty("address", address.toString());
                JsonArray references = new JsonArray();
                for (var it = currentProgram.getReferenceManager().getReferencesTo(address); it.hasNext();) {
                    var reference = it.next(); JsonObject item = new JsonObject();
                    item.addProperty("from", reference.getFromAddress().toString());
                    Function caller = currentProgram.getFunctionManager().getFunctionContaining(reference.getFromAddress());
                    if (caller != null) item.addProperty("function", caller.getEntryPoint().toString());
                    references.add(item);
                }
                literal.add("references", references); literals.add(literal);
            }
        }
        report.add("literals", literals);
        JsonArray globals = new JsonArray();
        for (String name : new String[]{"0x62fc9694", "0x62fc9698", "0x62fc969c"}) {
            JsonObject global = new JsonObject(); global.addProperty("address", name);
            JsonArray references = new JsonArray();
            for (var it = currentProgram.getReferenceManager().getReferencesTo(toAddr(name)); it.hasNext();) {
                var reference = it.next(); JsonObject item = new JsonObject();
                item.addProperty("from", reference.getFromAddress().toString());
                Function caller = currentProgram.getFunctionManager().getFunctionContaining(reference.getFromAddress());
                if (caller != null) item.addProperty("function", caller.getEntryPoint().toString());
                references.add(item);
            }
            global.add("references", references); globals.add(global);
        }
        report.add("global_references", globals);
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(report), StandardCharsets.UTF_8);
        println("AP01 native exports written: " + args[0]);
    }
}
