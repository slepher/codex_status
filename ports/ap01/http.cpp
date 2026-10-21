#include "http.h"
bool ap01ServeHttp(Ap01Application &app,const Ap01Stream &io,uint64_t now,uint64_t deadline) {
    return ap01ServeNativeHttp0041(&app,[](void *p,const Ap01HttpRequest &r,uint64_t n){return ((Ap01Application *)p)->businessHttp(r,n);},false,io,now,deadline);
}
