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
 size_t getBytes(const char *key,void *out,size_t cap){auto &b=endpointNvs[key];if(endpointReadFail||b.size()>cap)return 0;memcpy(out,b.data(),b.size());return b.size();}
 size_t putBytes(const char *key,const void *data,size_t size){if(endpointWriteFail)return 0;const auto *b=(const uint8_t *)data;endpointNvs[key]=std::vector<uint8_t>(b,b+size);return endpointCommitError?0:size;}
 String getString(const char *key,const char *fallback){auto &b=endpointNvs[key];return b.empty()?String(fallback):String(std::string(b.begin(),b.end()));}
 uint32_t getULong(const char *key,uint32_t value){auto &b=endpointNvs[key];if(b.empty())return value;uint32_t out=0;for(size_t i=0;i<b.size();++i)out|=uint32_t(b[i])<<(8*i);return out;}
 uint16_t getUShort(const char *key,uint16_t value){return getULong(key,value);}uint8_t getUChar(const char *key,uint8_t value){return getULong(key,value);}
 bool remove(const char *key){endpointNvs.erase(key);return true;}void clear(){endpointNvs.clear();}
};
