#include "ap01_template_page.h"
#include "ap01_template_display.h"
#include "template_renderer.h"
#include <memory>
#include <vector>

struct Activation { unsigned active; bool fail; };
static bool activate(void *user, unsigned index) {
    Activation *a = (Activation *)user;
    if (a->fail) return false;
    a->active = index;
    return true;
}
extern "C" int codex_ap01_page_check() {
    // Interleave LCD and EPD owners: neither canvas settings nor frame pointers
    // may leak through the legacy singleton when a native renderer is used.
    PAINT lcdPaint = {}, epdPaint = {};
    TemplateRenderer lcd(lcdPaint, 320, 240), epd(epdPaint, 200, 200);
    std::vector<uint8_t> lcdFrame(38400, 0), epdFrame(5000, 0);
    lcd.Paint_NewImage(lcdFrame.data(), 320, 240, 0, 0);
    lcd.Paint_SetScale(16);
    epd.Paint_NewImage(epdFrame.data(), 200, 200, 0, 0);
    epd.Paint_Clear(0xff);
    std::unique_ptr<CtTemplate> lcdCt(new CtTemplate), epdCt(new CtTemplate);
    String error;
    const char *lcdJson = R"({"schema":1,"id":"lcd","canvas":{"w":320,"h":240},"render_target":"lcd-jd9853-320x240-palette4","elements":[{"type":"rect","rect":[0,0,320,240],"fill":true,"color":"orange"}]})";
    const char *epdJson = R"({"schema":1,"id":"epd","canvas":{"w":200,"h":200},"elements":[{"type":"rect","rect":[0,0,200,200],"fill":true,"color":"black"}]})";
    if (!lcd.tplCompile(lcdJson, *lcdCt, error) || !epd.tplCompile(epdJson, *epdCt, error)) return __LINE__;
    if (lcd.tplCompile(epdJson, *epdCt, error) || epd.tplCompile(lcdJson, *epdCt, error)) return __LINE__;
    if (!epd.tplCompile(epdJson, *epdCt, error)) return __LINE__;
    TplEnv env;
    if (!lcd.tplDrawCt(*lcdCt, "{}", env) || !epd.tplDrawCt(*epdCt, "{}", env)) return __LINE__;
    auto epdBefore = epdFrame;
    if (!lcd.tplDrawCt(*lcdCt, "{}", env) || epdBefore != epdFrame) return __LINE__;
    for (uint8_t v : lcdFrame) if (v != 0x33) return __LINE__;
    if (epdFrame.front() == 0xff) return __LINE__;
    for (unsigned count = 1; count <= 8; ++count) {
        Activation a = {0, false};
        Ap01TemplatePage page = {false, 0};
        if (!ap01TemplateEnter(&page,count,0,1,activate,&a)) return __LINE__;
        for (unsigned i=1; i<count; ++i)
            if (ap01TemplateRotate(&page,count,1,activate,&a)!=AP01_TEMPLATE_CHANGED || page.index!=i || a.active!=i) return __LINE__;
        if (ap01TemplateRotate(&page,count,1,activate,&a)!=AP01_TEMPLATE_EXIT_NEXT || page.focused) return __LINE__;
        // Leaving for native pages retains the platform's last active template.
        if (a.active!=count-1 || !ap01TemplateEnter(&page,count,a.active,-1,activate,&a)) return __LINE__;
        for (unsigned i=count-1; i>0; --i)
            if (ap01TemplateRotate(&page,count,-1,activate,&a)!=AP01_TEMPLATE_CHANGED || page.index!=i-1) return __LINE__;
        if (ap01TemplateRotate(&page,count,-1,activate,&a)!=AP01_TEMPLATE_EXIT_PREVIOUS) return __LINE__;
        a.fail=true;
        if (count>1 && ap01TemplateEnter(&page,count,0,-1,activate,&a)) return __LINE__;
        if (page.focused) return __LINE__;
        page.focused=true; page.index=0;
        if (count>1 && (ap01TemplateRotate(&page,count,1,activate,&a)!=AP01_TEMPLATE_REJECTED || page.index!=0 || a.active!=0)) return __LINE__;
    }
    Activation a={0,false}; Ap01TemplatePage page={false,0};
    if (ap01TemplateEnter(&page,0,0,1,activate,&a) || ap01TemplateEnter(&page,9,0,1,activate,&a)) return __LINE__;
    std::vector<uint8_t> frame(38400,0x23);
    uint16_t row[320];
    if (!ap01ConvertRow(frame.data(),frame.size(),239,row,320) || row[0]!=ap01Palette565(2) || row[1]!=ap01Palette565(3)) return __LINE__;
    frame[0]=0xf0; row[0]=0x1234;
    if (!ap01ConvertRow(frame.data(),frame.size(),0,row,320) || row[0]!=ap01Palette565(15)) return __LINE__;
    if (ap01ConvertRow(frame.data(),frame.size()-1,0,row,320) || ap01ConvertRow(frame.data(),frame.size(),240,row,320)) return __LINE__;
    return 0;
}
