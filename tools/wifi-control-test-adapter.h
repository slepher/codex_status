// Host fixtures describe operations by name; production accepts numeric opcodes only.
static String testWifiControlCommand(JsonDocument &input, const WifiControlContext &ctx,
                                     uint64_t now, const String &legacy=String("[]")) {
    static const char *names[]={"snapshot","apply","result","result_ack","match",
        "active","trial","activate","setup_status","setup_cancel","scheme_get"};
    static const uint16_t codes[]={14,15,16,17,18,28,29,30,31,32,34};
    String name=input["op"]|""; if(name.startsWith("wifi_"))name=name.substring(5);
    uint16_t code=0;
    for(size_t i=0;i<sizeof(codes)/sizeof(codes[0]);++i)if(name==names[i])code=codes[i];
    JsonDocument payload;payload.set(input);payload.remove("op");
    return wifiControlCommand(code,payload,ctx,now,legacy);
}
