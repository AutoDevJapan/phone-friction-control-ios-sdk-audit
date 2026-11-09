#import <XCTest/XCTest.h>
@interface ProductPersistenceTests : XCTestCase
@end
@implementation ProductPersistenceTests
- (void)keepScreenshot:(NSString *)name app:(XCUIApplication *)app {
    XCTAttachment *a = [XCTAttachment attachmentWithScreenshot:app.screenshot];
    a.name = name;
    a.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:a];
}
- (void)testChangeLimit {
    XCUIApplication *app = [[XCUIApplication alloc] initWithBundleIdentifier:@"com.autodevjapan.phonefrictioncontrol.simulator"];
    [app launch];
    XCTAssertTrue([app waitForState:XCUIApplicationStateRunningForeground timeout:20]);
    [self keepScreenshot:@"home-before" app:app];
    [[app coordinateWithNormalizedOffset:CGVectorMake(0.918, 0.11)] tap];
    [NSThread sleepForTimeInterval:1];
    [self keepScreenshot:@"settings-before" app:app];
    if (app.sliders.firstMatch.exists && app.sliders.firstMatch.hittable) {
        [app.sliders.firstMatch adjustToNormalizedSliderPosition:0.7];
    } else {
        // Fixed geometry of the already observed product UI; file values are
        // checked independently by the shell. A returned tap is not a pass.
        [[app coordinateWithNormalizedOffset:CGVectorMake(0.25, 0.235)] tap];
    }
    [NSThread sleepForTimeInterval:3];
    [self keepScreenshot:@"settings-after" app:app];
}
@end
