"""Original RV32/LVGL unchanged-page re-entry regression; private QEMU only.
Use a one-template Bundle and precreated output/NOR with the normal runner args.
Radio/socket/input are explicit existing fixtures, not physical timing proof.
"""
from pathlib import Path
import sys
runner=Path(__file__).with_name('run-ap01-native-qemu.py')
source=runner.read_text(encoding='utf-8')
def replace(old,new):
    global source
    assert source.count(old)==1,'native runner version mismatch: '+old[:60]
    source=source.replace(old,new)
replace("        self.report['native_application_stage']='visual-cases'", """        self.report['native_application_stage']='entry-cache'
        assert len(status['template_ids'])==1,'use a one-template fixture'
        checks=[]
        def drawn():
            until=time.monotonic()+30
            while time.monotonic()<until:
                value=frame(1,'/api/status',{})
                if value['display_state']=='displayed':return value
                time.sleep(.1)
            raise TimeoutError('entry frame not drawn')
        def roundtrip(outkey,backkey,page):
            prior=drawn();self.input(0x11,outkey)
            assert self.action(observe=True)['page']==page
            self.input(0x11,backkey);after=drawn()
            assert after['active_context_id']==prior['active_context_id']
            assert after['frame_generation']==prior['frame_generation'],('unchanged re-entry redrew',page,prior['frame_generation'],after['frame_generation'])
            assert after['frame_crc']==prior['frame_crc']
            checks.append({'stock_page':page,'generation_unchanged':True,'context_unchanged':True})
        roundtrip(0x13,0x14,6);roundtrip(0x14,0x13,7)
        prior=drawn();self.input(0x11,0x13)
        changed={**values,'buckets[codex].5h.remaining':41}
        fields=[{'i':r['index'],'k':r['field'],'v':changed.get(r['field']),'q':'fresh' if r['field'] in changed else 'missing'} for r in status['data_contract']['requirements'] if not r['local']]
        fingerprint=''.join(f'{f["i"]}:{f["k"]}:'+('~' if f['v'] is None else json.dumps(f['v'],ensure_ascii=False,separators=(',',':')))+f':{f["q"]};' for f in fields).encode()
        frame(2,'/api/data',{'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,'request_id':'entry-offscreen-data','active_context_id':prior['active_context_id'],'seq':2,'crc':f'{zlib.crc32(fingerprint):08x}','fields':fields})
        until=time.monotonic()+30
        while time.monotonic()<until:
            if self.action(observe=True)['generation']>prior['frame_generation']:break
            time.sleep(.1)
        else:raise TimeoutError('offscreen updated frame not presented')
        off=frame(1,'/api/status',{});assert off['display_state']=='pending' and self.action(observe=True)['page']==6
        self.input(0x11,0x14);after=drawn()
        assert after['frame_generation']==off['frame_generation'] and after['applied_seq']==2 and after['frame_crc']!=prior['frame_crc']
        checks.append({'updated_offscreen_frame':True,'no_extra_entry_redraw':True,'actual_draw_ack':True})
        self.input(0x11,0x13)
        replacement=json.loads(json.dumps(bundle));replacement['job_id']='entry-replace'
        raw=json.dumps(replacement,ensure_ascii=False,separators=(',',':')).encode()
        envelope={'bridge_id':principal,'device_mac':identity,'session_nonce':nonce,'request_id':'entry-replace','length':len(raw),'content_crc':f'{zlib.crc32(raw):08x}'}
        frame(10,'/api/bundle/begin',envelope)
        for at in range(0,len(raw),3072):frame(11,f'/api/bundle/chunk?request_id=entry-replace&session_nonce={nonce}&offset={at}',raw[at:at+3072])
        frame(12,'/api/bundle/commit',envelope)
        until=time.monotonic()+30
        while time.monotonic()<until:
            if self.action(observe=True)['generation']>after['frame_generation']:break
            time.sleep(.1)
        else:raise TimeoutError('replacement frame not presented')
        off=frame(1,'/api/status',{});assert off['active_context_id']!=after['active_context_id'] and self.action(observe=True)['page']==6
        self.input(0x11,0x14);fresh=drawn()
        assert fresh['frame_generation']==off['frame_generation'] and fresh['frame_crc']==off['frame_crc'] and fresh['active_context_id']==off['active_context_id']
        checks.append({'replacement_invalidated_old_frame':True,'replacement_frame_reused':True})
        self.report['native_entry_cache']={'passed':True,'checks':checks,'physical_timing_verified':False}
        self.snapshot('entry-cache');self.report['native_application_stage']='complete';return
        self.report['native_application_stage']='visual-cases'""")
replace("functional=report.get('native_firmware_protocol_passed') if firmware_only else report['native_application_smoke_passed']","functional=report.get('native_entry_cache',{}).get('passed',False)")
if '--dump-variant' in sys.argv:
    Path(sys.argv[sys.argv.index('--output')+1]).with_suffix('.variant.py').write_text(source,encoding='utf-8');raise SystemExit(0)
sys.argv[0]=str(runner)
exec(compile(source,str(runner),'exec'),{'__name__':'__main__','__file__':str(runner)})
