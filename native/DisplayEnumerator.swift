import CoreGraphics
import Foundation

public struct QmvDisplayInfo {
    public let displayID: CGDirectDisplayID
    public let width: Int
    public let height: Int
    public let refreshRateHz: Double
    public let isMain: Bool
    public let isBuiltin: Bool
    public let pixelWidth: Int
    public let pixelHeight: Int
}

public final class QmvDisplayEnumerator {

    public static func enumerateAll() -> [QmvDisplayInfo] {
        var displayCount: UInt32 = 0
        let initialResult = CGGetActiveDisplayList(0, nil, &displayCount)
        guard initialResult == .success, displayCount > 0 else {
            return []
        }

        var displayIDs = [CGDirectDisplayID](repeating: 0, count: Int(displayCount))
        let listResult = CGGetActiveDisplayList(displayCount, &displayIDs, &displayCount)
        guard listResult == .success else {
            return []
        }

        return displayIDs.map { buildInfo(for: $0) }
    }

    public static func mainDisplay() -> QmvDisplayInfo {
        let mainID = CGMainDisplayID()
        return buildInfo(for: mainID)
    }

    private static func buildInfo(for displayID: CGDirectDisplayID) -> QmvDisplayInfo {
        let width = CGDisplayPixelsWide(displayID)
        let height = CGDisplayPixelsHigh(displayID)
        let isMain = CGDisplayIsMain(displayID) != 0
        let isBuiltin = CGDisplayIsBuiltin(displayID) != 0

        var refreshRate: Double = 60.0
        if let mode = CGDisplayCopyDisplayMode(displayID) {
            let rate = mode.refreshRate
            if rate > 0.0 {
                refreshRate = rate
            }
            return QmvDisplayInfo(
                displayID: displayID,
                width: width,
                height: height,
                refreshRateHz: refreshRate,
                isMain: isMain,
                isBuiltin: isBuiltin,
                pixelWidth: mode.pixelWidth,
                pixelHeight: mode.pixelHeight
            )
        }

        return QmvDisplayInfo(
            displayID: displayID,
            width: width,
            height: height,
            refreshRateHz: refreshRate,
            isMain: isMain,
            isBuiltin: isBuiltin,
            pixelWidth: width,
            pixelHeight: height
        )
    }

    public static func listAvailableRefreshRates(for displayID: CGDirectDisplayID) -> [Double] {
        guard let modes = CGDisplayCopyAllDisplayModes(displayID, nil) as? [CGDisplayMode] else {
            return []
        }

        let rates = modes.map { $0.refreshRate }.filter { $0 > 0.0 }
        return Array(Set(rates)).sorted()
    }
}

@_cdecl("qmv_swift_get_display_count")
public func qmv_swift_get_display_count() -> Int32 {
    return Int32(QmvDisplayEnumerator.enumerateAll().count)
}

@_cdecl("qmv_swift_get_main_display_width")
public func qmv_swift_get_main_display_width() -> Int32 {
    return Int32(QmvDisplayEnumerator.mainDisplay().pixelWidth)
}

@_cdecl("qmv_swift_get_main_display_height")
public func qmv_swift_get_main_display_height() -> Int32 {
    return Int32(QmvDisplayEnumerator.mainDisplay().pixelHeight)
}

@_cdecl("qmv_swift_get_main_display_refresh_rate")
public func qmv_swift_get_main_display_refresh_rate() -> Double {
    return QmvDisplayEnumerator.mainDisplay().refreshRateHz
}
