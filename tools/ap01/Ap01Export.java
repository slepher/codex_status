// Explicit seed analysis of an AP01 raw import; output is Ghidra evidence, not REA.
// @category AP01
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.Reference;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.charset.StandardCharsets;
import java.nio.file.StandardOpenOption;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.List;

public class Ap01Export extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 2) throw new IllegalArgumentException("output-file entry-address ...");
        String digest = currentProgram.getExecutableSHA256();
        if (!"972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9".equals(digest)
                && !"8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb".equals(digest))
            throw new IllegalArgumentException("not a verified AP01 input");
        Path output = Path.of(args[0]);
        if (!Files.isDirectory(output.getParent()) || Files.exists(output))
            throw new IllegalArgumentException("precreate parent; output must be new");
        for (MemoryBlock block : currentProgram.getMemory().getBlocks()) {
            block.setRead(true);
            block.setWrite(false);
            block.setExecute(true);
            currentProgram.getProgramContext().setValue(currentProgram.getRegister("gp"),
                block.getStart(), block.getEnd(), new BigInteger("62fc34c0", 16));
        }
        List<Address> seeds = new ArrayList<>();
        for (int i = 1; i < args.length; i++) {
            Address entry = toAddr(args[i]);
            seeds.add(entry);
            disassemble(entry);
            if (getFunctionAt(entry) == null) createFunction(entry, "candidate_" + entry);
        }
        analyzeChanges(currentProgram);
        StringBuilder report = new StringBuilder();
        report.append("engine=Ghidra ").append(getGhidraVersion()).append("\n");
        report.append("language=").append(currentProgram.getLanguageID()).append("\n");
        report.append("sha256=").append(currentProgram.getExecutableSHA256()).append("\n");
        report.append("functions=").append(currentProgram.getFunctionManager().getFunctionCount()).append("\n");
        report.append("Limits: raw flash image only; RAM initialization mapping and vendor extensions unverified.\n");
        DecompInterface decompiler = new DecompInterface();
        decompiler.openProgram(currentProgram);
        try {
            for (Address seed : seeds) {
                Function function = getFunctionAt(seed);
                report.append("\n=== seed ").append(seed).append(" ===\n");
                if (function == null) { report.append("No function recovered\n"); continue; }
                report.append("body=").append(function.getBody()).append("\n");
                for (Function called : function.getCalledFunctions(monitor))
                    report.append("calls ").append(called.getEntryPoint()).append(" ").append(called.getName()).append("\n");
                for (Reference ref : getReferencesTo(seed))
                    report.append("incoming ").append(ref.getFromAddress()).append(" ").append(ref.getReferenceType()).append("\n");
                DecompileResults result = decompiler.decompileFunction(function, 60, monitor);
                if (result.decompileCompleted() && result.getDecompiledFunction() != null)
                    report.append(result.getDecompiledFunction().getC());
                else report.append("DECOMPILATION FAILED ").append(result.getErrorMessage()).append("\n");
            }
        } finally { decompiler.dispose(); }
        Files.writeString(output, report.toString(), StandardCharsets.UTF_8, StandardOpenOption.CREATE_NEW);
        println("AP01 report saved: " + output);
    }
}
