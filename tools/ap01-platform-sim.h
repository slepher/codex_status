/* Live NuttX hostfs adapter. Network transactions execute the shared device
 * cores in device-sim; only UI messages/pixels/events cross this private port.
 * Never ship hostfs fixture paths as a physical AP01 network protocol. */
#pragma once
#include "ap01_template_lvgl.h"
#include <stdlib.h>
#include <unistd.h>

static Ap01TemplateImage g_ap01_template_image;
static unsigned g_ap01_template_active;
static struct {
  unsigned long long boot, generation, serial, drawn, acknowledged;
  uint32_t crc, revision;
  unsigned count, active, remaining;
  bool pairing, was_pairing, pair_drawn;
  char request[65], code[5], name[97];
} g_ap01_live;

static void ap01_live_event(const char *command) {
  FILE *file=fopen("/ap01/live-events.txt","a+");
  if(!file)return;
  if(fseek(file,0,SEEK_END)==0 && ftell(file)>4096){fclose(file);file=fopen("/ap01/live-events.txt","w");}
  if(!file)return;
  fprintf(file,"%llu %s\n",++g_ap01_live.serial,command);fflush(file);fclose(file);
}
static bool ap01_live_line(FILE *file,char *out,size_t cap) {
  if(!fgets(out,cap,file))return false;
  size_t n=strlen(out);if(!n)return false;
  if(out[n-1]=='\n')out[--n]=0;else if(fgetc(file)!='\n')return false;
  if(n&&out[n-1]=='\r')out[--n]=0;
  return true;
}
static bool ap01_live_read(void) {
  FILE *file=fopen("/ap01/live-ui.txt","r");if(!file)return false;
  char line[128];
  unsigned long long boot,gen;unsigned crc,pair,rev,remaining,count,active;
  char request[65],code[5],name[97];
  bool ok=ap01_live_line(file,line,sizeof(line))&&!strcmp(line,"AP01UI1");
  if(ok)ok=fscanf(file,"%llu\n%llu\n%u\n%u\n",&boot,&gen,&crc,&pair)==4;
  if(ok)ok=ap01_live_line(file,request,sizeof(request))&&fscanf(file,"%u\n",&rev)==1;
  if(ok)ok=ap01_live_line(file,code,sizeof(code))&&ap01_live_line(file,name,sizeof(name));
  if(ok)ok=fscanf(file,"%u\n%u\n%u\n",&remaining,&count,&active)==3;
  fclose(file);if(!ok||count>8||(count&&active>=count)||pair>1)return false;
  bool changed=g_ap01_live.boot!=boot||g_ap01_live.generation!=gen||g_ap01_live.pairing!=(pair!=0)||
    g_ap01_live.revision!=rev||g_ap01_live.remaining!=remaining;
  if(g_ap01_live.boot!=boot){g_ap01_live.serial=0;g_ap01_live.acknowledged=0;}
  g_ap01_live.boot=boot;g_ap01_live.generation=gen;g_ap01_live.crc=crc;
  g_ap01_live.pairing=pair!=0;g_ap01_live.revision=rev;g_ap01_live.remaining=remaining;
  g_ap01_live.count=count;g_ap01_live.active=active;g_ap01_template_active=active;
  if(!count)g_ap01_template_page.focused=false;
  else if(g_ap01_template_page.focused)g_ap01_template_page.index=(uint8_t)active;
  strcpy(g_ap01_live.request,request);strcpy(g_ap01_live.code,code);strcpy(g_ap01_live.name,name);
  return changed;
}
static unsigned ap01_template_sim_count(void) {return g_ap01_live.count;}
static bool ap01_template_sim_activate(void *unused,unsigned index) {
  (void)unused;char command[64];snprintf(command,sizeof(command),"activate %u",index);ap01_live_event(command);
  // A synchronous local activation callback must wait for the original command
  // result. A bounded timeout leaves focus unchanged, never reports success.
  unsigned long long event=g_ap01_live.serial;
  for(unsigned i=0;i<50;i++) {
    FILE *file=fopen("/ap01/live-activate-ack.txt","r");
    unsigned long long ack=0;unsigned applied=0,active=99;
    if(file){fscanf(file,"%llu %u %u",&ack,&applied,&active);fclose(file);}
    if(ack==event){if(applied&&active==index){ap01_live_read();return true;}return false;}
    usleep(10000);
  }
  printf("AP01 activation result unknown; event=%llu\n",event);return false;
}
static uint32_t ap01_live_crc(const uint8_t *p,size_t n) {
  uint32_t c=~0u;while(n--){c^=*p++;for(unsigned i=0;i<8;i++)c=(c>>1)^((0u-(c&1))&0xedb88320u);}return ~c;
}
static void ap01_template_sim_draw(void) {
  if(!lv_image_cache_is_enabled())lv_image_cache_resize(4096,true);
  FILE *file=fopen("/ap01/live-frame.palette4","rb");uint8_t *pixels=ap01TemplateImageCandidate();
  bool ok=file&&pixels&&fread(pixels,1,38400,file)==38400&&fgetc(file)==EOF;
  if(file)fclose(file);
  if(ok)ok=ap01_live_crc(pixels,38400)==g_ap01_live.crc;
  if(ok)ok=ap01TemplateImageCommit(&g_ap01_template_image,g_ap01_ui_root,pixels);
  if(ok)g_ap01_live.drawn=g_ap01_live.generation;
  else {ap01TemplateImageDiscard(pixels);ap01_ui_label(g_ap01_ui_root,"Template unavailable",40,100,lv_color_white(),LV_FONT_DEFAULT);}
}
static void ap01_live_pair_draw(void) {
  g_ap01_live.pair_drawn=false;
  if(!g_ap01_live.pairing||ap01_ui_boot_active()||ap01_ui_saver_active())return;
  lv_obj_t *panel=lv_obj_create(g_ap01_ui_root);lv_obj_set_pos(panel,8,12);lv_obj_set_size(panel,304,216);
  lv_obj_set_style_bg_color(panel,lv_color_hex(0x171b20),0);lv_obj_set_style_bg_opa(panel,LV_OPA_COVER,0);
  lv_obj_set_style_border_color(panel,lv_color_hex(0x52dfce),0);lv_obj_set_style_border_width(panel,1,0);
  ap01_ui_label(panel,"Pair with this computer?",12,8,lv_color_white(),LV_FONT_DEFAULT);
  ap01_ui_label(panel,g_ap01_live.name,12,32,lv_color_hex(0xaaaaaa),LV_FONT_DEFAULT);
  lv_obj_t *code=ap01_ui_label(panel,g_ap01_live.code,0,56,lv_color_white(),&ap01_rom_temperature);
  lv_obj_set_width(code,272);lv_obj_set_style_text_align(code,LV_TEXT_ALIGN_CENTER,0);
  ap01_ui_label(panel,"Press knob to confirm",12,127,lv_color_white(),LV_FONT_DEFAULT);
  char text[64];snprintf(text,sizeof(text),"Turn to cancel  /  %us",g_ap01_live.remaining);
  ap01_ui_label(panel,text,12,153,lv_color_hex(0xaaaaaa),LV_FONT_DEFAULT);
  g_ap01_live.pair_drawn=true;
}
static void ap01_live_after_refresh(void) {
  lv_refr_now(NULL);
  char command[128];
  if(g_ap01_live.pair_drawn) {
    snprintf(command,sizeof(command),"displayed %s %u",g_ap01_live.request,g_ap01_live.revision);
    if(!g_ap01_live.was_pairing){ap01_live_event(command);g_ap01_live.was_pairing=true;}
  } else g_ap01_live.was_pairing=false;
  if(!g_ap01_live.pairing&&g_ap01_live.drawn>g_ap01_live.acknowledged&&
      g_ap01_template_page.focused&&!ap01_ui_saver_active()) {
    snprintf(command,sizeof(command),"frame %llu",g_ap01_live.drawn);ap01_live_event(command);
    g_ap01_live.acknowledged=g_ap01_live.drawn;
  }
}
static bool ap01_live_input(lv_event_t *event) {
  if(!g_ap01_live.pairing)return false;
  lv_event_code_t code=lv_event_get_code(event);
  if(code==LV_EVENT_PRESSED)ap01_live_event("press");
  else if(code==LV_EVENT_RELEASED)ap01_live_event("release");
  else if(code==LV_EVENT_KEY) {
    uint32_t key=lv_event_get_key(event);
    if(key==LV_KEY_LEFT||key==LV_KEY_RIGHT)ap01_live_event("cancel");
  }
  return code==LV_EVENT_PRESSED||code==LV_EVENT_RELEASED||code==LV_EVENT_KEY||
    code==LV_EVENT_SHORT_CLICKED||code==LV_EVENT_LONG_PRESSED;
}
static void ap01_template_sim_tick(void) {
  if(ap01_live_read() && (g_ap01_live.pairing||g_ap01_live.was_pairing||g_ap01_template_page.focused))
    g_ap01_ui_redraw_pending=true;
}
