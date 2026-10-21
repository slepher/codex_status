"""Cross-compile AP01 binding and owned rendering cores; not a flashable ROM.



The caller must precreate --out with elevation per AGENTS. This checks the real

RV32 ilp32f objects, writable sections, undefined runtime imports and stack usage.

An audit object is deliberately not packaged as a completed AP01 application.

"""

from pathlib import Path

import argparse, hashlib, json, os, struct, subprocess



repo = Path(__file__).resolve().parents[1]
platform = repo.parent/'cuktech-ap01-firmware/tooling/firmware-modules'

parser = argparse.ArgumentParser()

parser.add_argument('--out', type=Path, default=repo/'artifacts/ap01-platform-integration/device')

parser.add_argument('--screen-host-probe',action='store_true')

args = parser.parse_args()

assert args.out.is_dir(), 'Precreate the output directory with elevation'

tool = Path(os.environ['USERPROFILE'])/'.platformio/packages/toolchain-riscv32-esp/bin/riscv32-esp-elf'

json_header = repo/'.pio/libdeps/esp32-s3-epaper-154g/ArduinoJson/src'

assert json_header.joinpath('ArduinoJson.h').is_file()

flags = ['-march=rv32imafc', '-mabi=ilp32f', '-Os', '-std=c++17', '-ffreestanding', '-fno-builtin',

         '-fno-exceptions', '-fno-rtti', '-fno-pic', '-mno-relax', '-msmall-data-limit=0',

         '-ffunction-sections', '-fdata-sections', '-fstack-usage', '-nostdlib',

         '-DARDUINOJSON_ENABLE_STD_STRING=0', '-DARDUINOJSON_ENABLE_STD_STREAM=0',

         '-DARDUINOJSON_ENABLE_ARDUINO_STRING=0', '-I'+str(platform), '-I'+str(repo/'src'), '-I'+str(json_header)]

report = {'scope':'owned binding/render/Bundle/HTTP and recovered 0041 UI/VFS runtime RV32 audit; execution reported separately',

          'passed':False,

          'firmware_target':'cuktech-ap01-0041', 'flashable':False, 'hardware_verified':False,

          'compiler':subprocess.check_output([str(tool)+'-g++.exe','--version'],text=True).splitlines()[0],

          'flags':flags, 'objects':[]}

if args.screen_host_probe:flags+=['-DAP01_SCREEN_HOST_PROBE=1']

header_inputs=set()

for directory in (platform,repo/'src',repo/'ports/ap01',repo/'bridge/crates/render/shim',json_header):

    header_inputs.update(p for p in directory.rglob('*') if p.is_file() and p.suffix in ('.h','.hpp'))

report['input_headers']=[{'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(header_inputs)]

report['generator_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()

path=args.out/'native-core-audit.json'

path.write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')

sources = [platform/'native_io_0041.cpp', platform/'screen_host_0041.c', platform/'screen_port_0041.cpp', repo/'src/ap01_binding.cpp', repo/'src/ap01_pairing.cpp', repo/'ports/ap01/abi_budget.cpp']

sources += [platform/'native_vfs_0041.cpp', repo/'ports/ap01/business.cpp', repo/'ports/ap01/application.cpp', repo/'ports/ap01/http.cpp', platform/'native_calendar_0041.cpp', platform/'native_runtime_0041.cpp', repo/'ports/ap01/native_0041.cpp', repo/'ports/ap01/manifest_0041.cpp']

sources += [platform/'native_device_code_0041.cpp',platform/'native_wifi_profiles_0041.cpp',platform/'native_wifi_profiles_port_0041.cpp',platform/'native_diagnostic_0041.cpp',platform/'native_firmware_0041.cpp',platform/'native_ota_image_0041.c',platform/'native_slot_0041.cpp']

sources += [repo/'src'/name for name in ['template_engine.cpp', 'GUI_Paint.cpp', 'font_asset.cpp',

                                      'sha256.cpp', 'v2_runtime.cpp', 'v2_data_command.cpp', 'v2_bundle_command.cpp', 'v2_activate_command.cpp', 'v2_command_envelope.cpp', 'v2_status_snapshot.cpp', 'bundle_store.cpp', 'bundle_payload.cpp', 'font8.cpp', 'font12.cpp', 'font16.cpp', 'font20.cpp', 'font24.cpp']]

if args.screen_host_probe:sources += [platform/'test_screen_peer_0041.cpp']

for source in sources:

    output = args.out/(source.stem+'.o')

    source_flags = flags

    if source.name not in ('ap01_binding.cpp', 'ap01_pairing.cpp'):

        source_flags = [f for f in flags if f != '-DARDUINOJSON_ENABLE_ARDUINO_STRING=0']

        source_flags += ['-DCODEX_AP01_NATIVE=1', '-DCODEX_RENDER_NOTE4_FONTS=1',

                         '-DARDUINOJSON_ENABLE_ARDUINO_STRING=1',

                         '-I'+str(repo/'ports/ap01'), '-I'+str(repo/'bridge/crates/render/shim')]

    compiler='-gcc.exe' if source.suffix=='.c' else '-g++.exe'

    if source.suffix=='.c':source_flags=[f.replace('gnu++17','gnu11').replace('c++17','c11') for f in source_flags if f not in ('-fno-exceptions','-fno-rtti')]

    subprocess.run([str(tool)+compiler,*source_flags,'-c',str(source),'-o',str(output)],check=True)

    sizes = subprocess.check_output([str(tool)+'-size.exe','-A',str(output)],text=True)

    sections = []

    for line in sizes.splitlines():

        fields = line.split()

        if len(fields)==3 and fields[0].startswith('.'):

            name,size,address = fields[0],int(fields[1]),int(fields[2]); sections.append({'name':name,'bytes':size})

            forbidden = ('.data','.bss','.sdata','.sbss','.got','.tdata','.tbss',

                         '.init_array','.fini_array','.ctors','.dtors')

            assert not size or not name.startswith(forbidden), 'XIP writable state or implicit initializer: '+name

    imports = subprocess.check_output([str(tool)+'-nm.exe','-u',str(output)],text=True).splitlines()

    stack = output.with_suffix('.su').read_text(encoding='utf-8').splitlines()

    report['objects'].append({'file':str(output),'source':str(source),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),

                             'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'bytes':output.stat().st_size,

                             'flags':source_flags,'sections':sections,

                             'undefined_imports':[line.strip() for line in imports], 'stack':stack})

budget_file=args.out/'abi-budget.bin'

subprocess.run([str(tool)+'-objcopy.exe','-O','binary','--only-section=.ap01_budget',

                str(args.out/'abi_budget.o'),str(budget_file)],check=True)

pointer,state,record,count,header,maximum,frame=struct.unpack('<7I',budget_file.read_bytes())

assert pointer==4

report['budget']={'state_heap_bytes':state,'record_heap_bytes':record,'bindings_max':count,

                  'frame_bytes':frame,'persistent_empty_bytes':header,

                  'persistent_single_typical_bytes':header+5+16+16+4+32+32,

                  'persistent_eight_typical_bytes':header+8*(5+16+16+4+32+32),

                  'persistent_single_max_bytes':header+maximum,'persistent_eight_max_bytes':header+8*maximum,

                  'commit_heap_max_bytes':state+2*count*record+header+count*maximum,

                  'limitations':'allocator overhead, JSON/UI/Bundle allocations and native calling chains require separate audit'}

subprocess.run([str(tool)+'-objcopy.exe','-O','binary','--only-section=.ap01_render_budget',

                str(args.out/'abi_budget.o'),str(budget_file)],check=True)

paint,renderer,compiled = struct.unpack('<3I', budget_file.read_bytes())

report['budget'].update({'paint_state_bytes':paint,'renderer_state_bytes':renderer,

                        'compiled_template_bytes':compiled})

subprocess.run([str(tool)+'-objcopy.exe','-O','binary','--only-section=.ap01_firmware_budget',

                str(args.out/'native_firmware_0041.o'),str(budget_file)],check=True)

firmware,transaction,body,headers,receipt,scratch=struct.unpack('<6I',budget_file.read_bytes())

report['budget']['firmware']={'service_bytes':firmware,'transaction_bytes':transaction,

    'max_chunk_heap_bytes':body,'http_header_and_small_body_heap_bytes':headers,

    'max_persistent_record_bytes':receipt,'max_pending_plus_record_bytes':receipt*2,

    'native_write_scratch_stack_bytes':scratch}

# This is an ET_REL core, not a linked executable or a ROM. Merge objects to

# expose the actual remaining libc/libgcc imports rather than per-object calls.

partial = args.out/'ap01-shared-core.o'

subprocess.run([str(tool)+'-ld.exe','-r',*[str(args.out/(s.stem+'.o')) for s in sources],

                '-o',str(partial)],check=True)

report['combined'] = {'path':str(partial),'sha256':hashlib.sha256(partial.read_bytes()).hexdigest(),

                      'bytes':partial.stat().st_size,

                      'undefined_imports':subprocess.check_output([str(tool)+'-nm.exe','-u',str(partial)],text=True).splitlines()}

report['passed'] = True

path.write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')

print(json.dumps({'passed':True,'flashable':False,'report':str(path),'budget':report['budget']}))
