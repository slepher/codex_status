#import <CoreWLAN/CoreWLAN.h>
#import <CoreLocation/CoreLocation.h>
#import <CoreBluetooth/CoreBluetooth.h>
#import <AppKit/AppKit.h>
#include <string.h>

static void requestAuthorization(int request);
@interface WifiAuthorization : NSObject <CLLocationManagerDelegate, CBCentralManagerDelegate>
@property(nonatomic, strong) CLLocationManager *location;
@property(nonatomic, strong) CBCentralManager *bluetooth;
@property(nonatomic) BOOL locationRequestPending;
@property(nonatomic) BOOL bluetoothRequestPending;
@property(nonatomic) BOOL combinedRequestPending;
@property(nonatomic, strong) id activationObserver;
@end
@implementation WifiAuthorization
- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager {
    if (manager.authorizationStatus != kCLAuthorizationStatusNotDetermined) {
        self.locationRequestPending = NO;
        if (self.combinedRequestPending) requestAuthorization(3);
    }
}
- (void)centralManagerDidUpdateState:(CBCentralManager *)central { }
@end

// All manager ownership/request work stays on the app main run loop.
// Reading permissions never requests access or starts location updates.
static WifiAuthorization *authorization(void) {
    static WifiAuthorization *value;
    if (!value) {
        value = [WifiAuthorization new];
        value.location = [CLLocationManager new];
        value.location.delegate = value;
    }
    return value;
}
static NSString *locationState(void) {
    if (![CLLocationManager locationServicesEnabled]) return @"disabled";
    switch (authorization().location.authorizationStatus) {
        case kCLAuthorizationStatusNotDetermined: return @"not_determined";
        case kCLAuthorizationStatusRestricted: return @"restricted";
        case kCLAuthorizationStatusDenied: return @"denied";
        case kCLAuthorizationStatusAuthorizedAlways: return @"allowed";
        default: return @"unavailable";
    }
}
static NSString *bluetoothState(void) {
    switch ([CBManager authorization]) {
        case CBManagerAuthorizationNotDetermined: return @"not_determined";
        case CBManagerAuthorizationRestricted: return @"restricted";
        case CBManagerAuthorizationDenied: return @"denied";
        case CBManagerAuthorizationAllowedAlways: return @"allowed";
        default: return @"unavailable";
    }
}
static void requestAuthorization(int request) {
    WifiAuthorization *value = authorization();
    if (request == 3) {
        value.combinedRequestPending = YES;
        // Continue from the delegate even if the user takes longer than the UI poll.
        if ([locationState() isEqual:@"not_determined"]) {
            if (value.locationRequestPending) return;
            request = 1;
        } else {
            value.combinedRequestPending = NO;
            request = 2;
        }
    }
    BOOL location = request == 1 && [locationState() isEqual:@"not_determined"] && !value.locationRequestPending;
    BOOL bluetooth = request == 2 && [CBManager authorization] == CBManagerAuthorizationNotDetermined && !value.bluetooth && !value.bluetoothRequestPending;
    if (!location && !bluetooth) return;
    if (location) value.locationRequestPending = YES;
    if (bluetooth) value.bluetoothRequestPending = YES;
    void (^ask)(void) = ^{
        if (value.locationRequestPending) [value.location requestWhenInUseAuthorization];
        if (value.bluetoothRequestPending && !value.bluetooth) {
            value.bluetooth = [[CBCentralManager alloc] initWithDelegate:value queue:nil
                options:@{CBCentralManagerOptionShowPowerAlertKey:@NO}];
            value.bluetoothRequestPending = NO;
        }
    };
    // A background/accessory app's when-in-use request can be ignored. Wait
    // for actual activation, rather than guessing when it has become active.
    if (NSApp.isActive) ask();
    else if (!value.activationObserver) {
        value.activationObserver = [[NSNotificationCenter defaultCenter]
            addObserverForName:NSApplicationDidBecomeActiveNotification object:NSApp queue:[NSOperationQueue mainQueue]
            usingBlock:^(NSNotification *notification) {
                [[NSNotificationCenter defaultCenter] removeObserver:value.activationObserver];
                value.activationObserver = nil;
                ask();
            }];
        [NSApp activateIgnoringOtherApps:YES];
    }
}
static char *jsonResult(NSDictionary *value) {
    NSData *json = [NSJSONSerialization dataWithJSONObject:value options:0 error:nil];
    if (!json) return NULL;
    return strdup([[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding].UTF8String);
}
char *codex_permissions_read(int request) {
    __block char *result = NULL;
    void (^read)(void) = ^{
        @autoreleasepool {
            if (request) requestAuthorization(request);
            result = jsonResult(@{@"platform":@"macos", @"location":locationState(), @"bluetooth":bluetoothState()});
        }
    };
    if ([NSThread isMainThread]) read(); else dispatch_sync(dispatch_get_main_queue(), read);
    return result;
}
// CoreBluetooth's class property does not instantiate a central or prompt.
int codex_bluetooth_authorization(void) { return (int)[CBManager authorization]; }
int codex_privacy_open(int section) {
    __block BOOL opened = NO;
    void (^open)(void) = ^{
        NSString *url = section == 0 ? @"x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices" :
            section == 1 ? @"x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth" :
            section == 2 ? @"x-apple.systempreferences:com.apple.preference.security?Privacy" : nil;
        if (url) opened = [[NSWorkspace sharedWorkspace] openURL:[NSURL URLWithString:url]];
    };
    if ([NSThread isMainThread]) open(); else dispatch_sync(dispatch_get_main_queue(), open);
    return opened;
}
char *codex_host_wifi_read(int requestPermission) {
    __block char *result = NULL;
    void (^read)(void) = ^{
        @autoreleasepool {
            if (requestPermission) requestAuthorization(1);
            NSString *permission = locationState();
            NSMutableArray *networks = [NSMutableArray new];
            for (CWInterface *interface in [CWWiFiClient sharedWiFiClient].interfaces) {
                NSString *ssid = interface.ssid;
                if (ssid.length) [networks addObject:@{@"interface":interface.interfaceName ?: @"", @"ssid":ssid}];
            }
            NSString *state = networks.count > 1 ? @"ambiguous" : networks.count == 1 ? @"connected" :
                [permission isEqual:@"disabled"] ? @"location_disabled" :
                [permission isEqual:@"denied"] || [permission isEqual:@"restricted"] ? @"permission_denied" :
                [permission isEqual:@"not_determined"] ? @"permission_required" : @"no_wifi";
            result = jsonResult(@{@"state":state, @"networks":networks});
        }
    };
    if ([NSThread isMainThread]) read(); else dispatch_sync(dispatch_get_main_queue(), read);
    return result;
}
