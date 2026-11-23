"""Check AP01 App-specific health causes and first-failure retention, offline."""
from pathlib import Path
import subprocess
import tempfile

root=Path(__file__).resolve().parents[1]
source=r'''
#include "health_v3.h"
#include <cassert>
#include <climits>
int main(){
    using H=CodexAppHealthV3;
    H healthy;
    assert(healthy.sample(0,0,0,0)==1);
    assert(healthy.sample(3,15000,0,10000)==0);
    assert(healthy.sample(3,15001,0,15001)==H::BusinessStale);
    assert(healthy.sample(3,5001,5001,0)==H::DiscoveryStale);
    assert(healthy.sample(3,5,UINT32_MAX-4,UINT32_MAX-4)==0);
    for(auto code:{H::InitializingFailed,H::WorkerExited,H::BusinessStale,H::DiscoveryStale,
                  H::TcpListenFailed,H::UdpListenFailed,H::WriteRestoreFailed,H::UiRestoreFailed}){
        H h;h.fail(code);h.fail(H::WorkerExited);
        assert(h.sample(0,0,0,0)==code);
        assert(h.sample(3,1,1,1)==code);
    }
}
'''
with tempfile.TemporaryDirectory(prefix='ap01-health-') as directory:
    folder=Path(directory);path=folder/'test.cpp';path.write_text(source)
    binary=folder/'test'
    subprocess.run(['clang++','-std=c++17','-include','initializer_list','-Wall','-Wextra','-Werror',
                    '-fsanitize=address,undefined','-I'+str(root/'ports/ap01'),str(path),'-o',str(binary)],check=True)
    subprocess.run([str(binary)],check=True)
print('AP01 health: distinct reasons, heartbeat deadlines/wraparound and first-failure retention passed')
