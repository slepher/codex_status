#pragma once
#include <map>
#include <vector>
static std::map<std::string,std::vector<uint8_t>> endpointNvs;
static bool endpointWriteFail=false,endpointCommitError=false,endpointReadFail=false;
class Preferences {
public:
 bool begin(const char*,bool){return true;}void end(){}
 bool isKey(const char *key){return endpointNvs.find(key)!=endpointNvs.end();}
 size_t getBytesLength(const char *key){auto i=endpointNvs.find(key);return i==endpointNvs.end()?0:i->second.size();}
 size_t getBytes(const char *key,void *out,size_t cap){auto found=endpointNvs.find(key);if(found==endpointNvs.end())return 0;auto &b=found->second;if(endpointReadFail||b.size()>cap)return 0;memcpy(out,b.data(),b.size());return b.size();}
 size_t putBytes(const char *key,const void *data,size_t size){if(endpointWriteFail)return 0;const auto *b=(const uint8_t *)data;endpointNvs[key]=std::vector<uint8_t>(b,b+size);return endpointCommitError?0:size;}
 size_t getString(const char *key,char *out,size_t cap){auto i=endpointNvs.find(key);if(i==endpointNvs.end()||endpointReadFail||i->second.size()+1>cap)return 0;memcpy(out,i->second.data(),i->second.size());out[i->second.size()]=0;return i->second.size()+1;}
 uint32_t number(const char *key,uint32_t value,size_t width){auto i=endpointNvs.find(key);if(i==endpointNvs.end()||i->second.size()!=width)return value;uint32_t out=0;for(size_t n=0;n<width;++n)out|=uint32_t(i->second[n])<<(8*n);return out;}
 uint32_t getULong(const char *key,uint32_t value){return number(key,value,4);}
 uint16_t getUShort(const char *key,uint16_t value){return number(key,value,2);}uint8_t getUChar(const char *key,uint8_t value){return number(key,value,1);}
 bool remove(const char *key){endpointNvs.erase(key);return true;}void clear(){endpointNvs.clear();}
};
