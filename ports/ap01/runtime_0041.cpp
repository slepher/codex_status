// Freestanding support for the frozen RV32/ilp32f 0041 imports. All mutable
// allocations carry their length and are owned by the application, not XIP.
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <new>
#include <time.h>
#include <stdio.h>
#define ROM_FN(address,type) ((type)(uintptr_t)(address))
extern "C" {
void *malloc(size_t n) noexcept {
    if (n > SIZE_MAX-8) return nullptr;
    uint32_t *p = ROM_FN(0xa002fcc8, uint32_t *(*)(size_t))(n+8);
    if (!p) return nullptr;
    p[0] = n; p[1] = 0x4130314d; return p+2;
}
void free(void *p) noexcept {
    if (p) ROM_FN(0xa002e14e, void (*)(void *))((uint32_t *)p-2);
}
void *realloc(void *p, size_t n) noexcept {
    if (!p) return malloc(n);
    if (!n) { free(p); return nullptr; }
    size_t old = ((uint32_t *)p)[-2];
    if (n <= old) return p;
    void *next = malloc(n); if (!next) return nullptr;
    memcpy(next, p, old); free(p); return next;
}
void *calloc(size_t count,size_t n) noexcept {
    if (count && n > SIZE_MAX/count) return nullptr;
    n *= count; void *p = malloc(n); if (p) memset(p,0,n); return p;
}
void *memcpy(void *d,const void *s,size_t n) noexcept { auto *p=(uint8_t *)d;auto *q=(const uint8_t *)s;while(n--)*p++=*q++;return d; }
void *memmove(void *d,const void *s,size_t n) noexcept {
    auto *p=(uint8_t *)d;auto *q=(const uint8_t *)s;
    if ((uintptr_t)p <= (uintptr_t)q) return memcpy(d,s,n);
    p+=n;q+=n;while(n--)*--p=*--q;return d;
}
void *memset(void *d,int c,size_t n) noexcept { auto *p=(uint8_t *)d;while(n--)*p++=c;return d; }
int memcmp(const void *a,const void *b,size_t n) noexcept {auto *p=(const uint8_t *)a,*q=(const uint8_t *)b;while(n--){if(*p!=*q)return int(*p)-*q;++p;++q;}return 0;}
void *memchr(const void *s,int c,size_t n) noexcept {auto *p=(const uint8_t *)s;while(n--){if(*p==(uint8_t)c)return (void *)p;++p;}return nullptr;}
size_t strlen(const char *s) noexcept {const char *p=s;while(*p)++p;return p-s;}
int strcmp(const char *a,const char *b) noexcept {while(*a&&*a==*b){++a;++b;}return (unsigned char)*a-(unsigned char)*b;}
int strncmp(const char *a,const char *b,size_t n) noexcept {while(n--){if(*a!=*b||!*a)return (unsigned char)*a-(unsigned char)*b;++a;++b;}return 0;}
char *strcpy(char *a,const char *b) noexcept {char *p=a;while((*p++=*b++)){}return a;}
char *strchr(const char *s,int c) noexcept {do {if(*s==(char)c)return (char *)s;}while(*s++);return nullptr;}
char *strstr(const char *s,const char *needle) noexcept {if(!*needle)return (char *)s;for(;*s;++s){size_t i=0;while(needle[i]&&s[i]==needle[i])++i;if(!needle[i])return (char *)s;}return nullptr;}
long strtol(const char *s,char **end,int base) noexcept {
    const char *start=s;while(*s==' '||*s=='\t'||*s=='\n'||*s=='\r')++s;
    bool negative=*s=='-';if(*s=='+'||*s=='-')++s;
    if (!base) base=*s=='0'?8:10;
    if ((base==16||base==8) && s[0]=='0' && (s[1]=='x'||s[1]=='X')) {base=16;s+=2;}
    const char *digits=s;unsigned long value=0,limit=negative?0x80000000u:0x7fffffffu;bool overflow=false;
    while(*s){unsigned d=*s>='0'&&*s<='9'?*s-'0':*s>='a'&&*s<='z'?*s-'a'+10:*s>='A'&&*s<='Z'?*s-'A'+10:36;
        if(d>=(unsigned)base)break;if(value>(limit-d)/(unsigned)base)overflow=true;else if(!overflow)value=value*base+d;++s;}
    if(end)*end=(char *)(s==digits?start:s);if(overflow)value=limit;
    return negative?(long)(0u-value):(long)value;
}
// Newlib's libm may set errno. Use the running NuttX thread's real errno slot.
int *__errno() { return ROM_FN(0xa00267c0,int *(*)())(); }
time_t time(time_t *out) noexcept {
    int32_t value=ROM_FN(0xa019fc3e,int32_t (*)())();
    // The AP01 RTC stores Hong Kong civil seconds. Shared rendering expects
    // UTC epochs, including device.date; timestamp fields are already UTC.
    if(value>=8*60*60)value-=8*60*60;
    if(out)*out=value;return value;
}
size_t strftime(char *out, size_t length, const char *format, const struct tm *value) noexcept {
    if (!length) return 0;
    int n = !strcmp(format,"%H:%M") ? snprintf(out,length,"%02d:%02d",value->tm_hour,value->tm_min) :
        !strcmp(format,"%m/%d") ? snprintf(out,length,"%02d/%02d",value->tm_mon+1,value->tm_mday) :
        !strcmp(format,"%m-%d %H:%M") ? snprintf(out,length,"%02d-%02d %02d:%02d",value->tm_mon+1,value->tm_mday,value->tm_hour,value->tm_min) : -1;
    if (n < 0 || size_t(n) >= length) {out[0]=0;return 0;}return n;
}
}
namespace std { const nothrow_t nothrow{}; }
void *operator new(size_t n,const std::nothrow_t &) noexcept { return malloc(n); }
void *operator new[](size_t n,const std::nothrow_t &) noexcept { return malloc(n); }
void operator delete(void *p) noexcept {free(p);}
void operator delete[](void *p) noexcept {free(p);}
void operator delete(void *p,size_t) noexcept {free(p);}
void operator delete[](void *p,size_t) noexcept {free(p);}
