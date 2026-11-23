#pragma once
#include <atomic>
#include <stdint.h>

// App-specific health codes; ABI common codes remain -1 through -15.
struct CodexAppHealthV3 {
    enum Code:int32_t {InitializingFailed=-101,WorkerExited=-102,BusinessStale=-103,
        DiscoveryStale=-104,TcpListenFailed=-105,UdpListenFailed=-106,
        WriteRestoreFailed=-107,UiRestoreFailed=-108};
    std::atomic<int32_t> first{0};
    void fail(Code code){int32_t expected=0;first.compare_exchange_strong(expected,code);}
    int32_t sample(uint32_t workers,uint32_t now,uint32_t business,uint32_t discovery)const{
        int32_t failure=first.load();if(failure)return failure;
        if(workers!=3)return 1;
        if(uint32_t(now-business)>15000)return BusinessStale;
        if(uint32_t(now-discovery)>5000)return DiscoveryStale;
        return 0;
    }
};
