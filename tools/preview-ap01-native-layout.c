/* Host-only preview. Select one intact face per label; no glyph-level fallback. */
#include "fonts.h"
#include "src/misc/lv_text_private.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

static uint16_t framebuffer[320 * 240];
static uint16_t renderbuffer[320 * 240];
static lv_obj_t *root;
static const lv_font_t *native_label_font=&ap01_rom_montserrat14;
static const uint32_t white=0xf3f5f7, muted=0xaeb8c5, cyan=0x44d5e9,
                      orange=0xffb75b, track=0x263340;

static void flush(lv_display_t *d, const lv_area_t *a, uint8_t *pixels) {
    for(int y=a->y1;y<=a->y2;y++)
        memcpy(framebuffer+y*320+a->x1, pixels+(y-a->y1)*(a->x2-a->x1+1)*2,
               (a->x2-a->x1+1)*2);
    lv_display_flush_ready(d);
}

static bool covers_text(const lv_font_t *font,const char *value) {
    uint32_t index=0;
    while(value[index]) {
        uint32_t cp=lv_text_encoded_next(value,&index);
        lv_font_glyph_dsc_t glyph;
        if(!lv_font_get_glyph_dsc(font,&glyph,cp,0)||glyph.is_placeholder) return false;
    }
    return true;
}

static const lv_font_t *font_for(const char *text, int desired) {
    if(desired<=22) return covers_text(native_label_font,text)?native_label_font:&supplement_font;
    const lv_font_t *best=NULL;
    int distance=1000;
    for(unsigned i=0;i<sizeof(original_fonts)/sizeof(*original_fonts);i++) {
        const lv_font_t *f=original_fonts[i];
        int score=abs((int)f->line_height-desired);
        if(covers_text(f,text)&&score<distance) { best=f;distance=score; }
    }
    return best ? best : &supplement_font;
}

static lv_obj_t *text(const char *value,int x,int y,int width,int height,
                      int desired,uint32_t color,int align,bool ellipsis) {
    const lv_font_t *font=font_for(value,desired);
    assert(font->fallback==NULL && covers_text(font,value));
    if(strncmp(value,"SYNC ",5)==0) assert(font==native_label_font);
    lv_point_t size;
    lv_text_get_size(&size,value,font,0,0,1000,LV_TEXT_FLAG_NONE);
    assert(x>=0&&y>=0&&x+width<=320&&y+height<=240);
    assert(font->line_height<=height);
    if(!ellipsis&&size.x>width) fprintf(stderr,"Overflow %s: %ld > %d\n",value,(long)size.x,width);
    assert(ellipsis||size.x<=width);
    lv_obj_t *label=lv_label_create(root);
    lv_obj_set_style_text_font(label,font,0);
    lv_obj_set_style_text_color(label,lv_color_hex(color),0);
    lv_obj_set_style_text_align(label,align,0);
    lv_label_set_long_mode(label,ellipsis?LV_LABEL_LONG_DOT:LV_LABEL_LONG_CLIP);
    lv_obj_set_size(label,width,height);
    lv_label_set_text(label,value);
    lv_obj_set_pos(label,x,y+(height-font->line_height)/2);
    printf("%s: face=%s, line_height=%u, measured_width=%ld, box=%d,%d,%d,%d%s\n",
           value,font==native_label_font?"ROM-Montserrat14":font==&supplement_font?"YaHeiUI20":"original-large-digits",font->line_height,
           (long)size.x,x,y,width,height,ellipsis?", bounded ellipsis":"");
    return label;
}

static void rule(int x,int y,int width,int height) {
    lv_obj_t *o=lv_obj_create(root);
    lv_obj_remove_style_all(o);
    lv_obj_set_pos(o,x,y);lv_obj_set_size(o,width,height);
    lv_obj_set_style_bg_color(o,lv_color_hex(track),0);
    lv_obj_set_style_bg_opa(o,LV_OPA_COVER,0);
}

static void ring(int x,int y,int size,int value) {
    lv_obj_t *a=lv_arc_create(root);
    lv_obj_remove_style_all(a);
    lv_obj_set_pos(a,x,y);lv_obj_set_size(a,size,size);
    lv_obj_set_style_arc_color(a,lv_color_hex(track),LV_PART_MAIN);
    lv_obj_set_style_arc_color(a,lv_color_hex(cyan),LV_PART_INDICATOR);
    lv_obj_set_style_arc_width(a,4,LV_PART_MAIN);
    lv_obj_set_style_arc_width(a,4,LV_PART_INDICATOR);
    lv_obj_set_style_arc_rounded(a,true,LV_PART_INDICATOR);
    lv_arc_set_bg_angles(a,0,360);lv_arc_set_rotation(a,270);
    lv_arc_set_range(a,0,100);lv_arc_set_value(a,value);
    lv_obj_remove_flag(a,LV_OBJ_FLAG_CLICKABLE);
}

static void scene(const char *name,bool dual,bool monthly,bool long_account) {
    lv_obj_clean(root);
    printf("SCENE %s\n",name);
    /* Subscription + account move into the first row; time stays at right. */
    text("Pro",16,10,35,26,20,white,LV_TEXT_ALIGN_LEFT,false);
    text(long_account?"account-name-that-is-too-long":"Alex",60,10,176,26,20,muted,LV_TEXT_ALIGN_LEFT,true);
    text("14:36",244,10,60,26,20,white,LV_TEXT_ALIGN_RIGHT,false);
    rule(16,39,288,1);
    int cx=dual?78:160;
    ring(cx-77,47,154,75);
    text(monthly?"月额度剩余":"周额度剩余",cx-60,64,120,26,20,cyan,LV_TEXT_ALIGN_CENTER,false);
    text("75",cx-60,88,120,48,48,white,LV_TEXT_ALIGN_CENTER,false);
    text("%",cx-10,137,20,25,20,white,LV_TEXT_ALIGN_CENTER,false);
    text(dual?"7 天窗口":"10/24 14:30",cx-58,dual?166:154,116,24,19,muted,LV_TEXT_ALIGN_CENTER,false);
    if(dual) {
        rule(159,55,1,141);
        text("5 小时剩余",166,57,138,26,21,orange,LV_TEXT_ALIGN_LEFT,false);
        text("88",166,85,100,41,38,white,LV_TEXT_ALIGN_LEFT,false);
        text("%",233,99,25,26,20,white,LV_TEXT_ALIGN_LEFT,false);
        text("15:30 重置",166,128,138,26,20,muted,LV_TEXT_ALIGN_LEFT,false);
        text("周额度重置",166,154,138,24,19,cyan,LV_TEXT_ALIGN_LEFT,false);
        text("10/24 14:30",166,178,138,24,18,white,LV_TEXT_ALIGN_LEFT,false);
    }
    /* Previously two bottom rows, now one. */
    rule(16,204,288,1);
    text("SYNC 14:35",16,214,180,26,20,muted,LV_TEXT_ALIGN_LEFT,false);
    text("RC 2",240,214,64,26,20,orange,LV_TEXT_ALIGN_RIGHT,false);
    lv_refr_now(NULL);
    char path[80];snprintf(path,sizeof(path),"%s.rgb565",name);
    FILE *file=fopen(path,"wb");assert(file);
    assert(fwrite(framebuffer,2,320*240,file)==320*240);assert(fclose(file)==0);
}

int main(void) {
    lv_init();
    assert(native_label_font->line_height==16 && covers_text(native_label_font,"SYNC"));
    for(unsigned cp=32;cp<127;cp++) {
        char value[2]={(char)cp,0};assert(covers_text(native_label_font,value));
    }
    lv_display_t *display=lv_display_create(320,240);
    lv_display_set_color_format(display,LV_COLOR_FORMAT_RGB565);
    lv_display_set_buffers(display,renderbuffer,NULL,sizeof(renderbuffer),LV_DISPLAY_RENDER_MODE_FULL);
    lv_display_set_flush_cb(display,flush);
    root=lv_screen_active();lv_obj_remove_style_all(root);
    lv_obj_set_style_bg_color(root,lv_color_hex(0),0);
    lv_obj_set_style_bg_opa(root,LV_OPA_COVER,0);
    lv_obj_remove_flag(root,LV_OBJ_FLAG_SCROLLABLE);
    scene("dual",true,false,false);
    scene("weekly",false,false,false);
    scene("monthly",false,true,false);
    scene("long-account",true,false,true);
    lv_deinit();return 0;
}
