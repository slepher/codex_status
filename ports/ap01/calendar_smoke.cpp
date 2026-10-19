// Host check of the AP01 civil-time contract; native ABI is tested separately.
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <time.h>
#include <initializer_list>
bool ap01Calendar(time_t,struct tm &);
int main() {
    struct Case {time_t epoch;int year,month,day,hour,minute;};
    for(auto test:{Case{1792429140,2026,10,20,0,59},
                  Case{1798732800,2027,1,1,0,0},
                  Case{1709224200,2024,3,1,0,30},
                  Case{0,1970,1,1,8,0}}) {
        struct tm t{};assert(ap01Calendar(test.epoch,t));
        assert(t.tm_year+1900==test.year&&t.tm_mon+1==test.month&&t.tm_mday==test.day);
        assert(t.tm_hour==test.hour&&t.tm_min==test.minute);
    }
    struct tm t{};assert(!ap01Calendar(-1,t));
    assert(!ap01Calendar(time_t(UINT32_MAX)+1,t));
    puts("AP01 UTC+8: midnight, year/leap rollover, epoch and bounds passed");
}
