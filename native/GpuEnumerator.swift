import Metal
import Foundation

public struct QmvGpuInfo {
    public let name: String
    public let vendorID: UInt32
    public let deviceID: UInt32
    public let isLowPower: Bool
    public let isRemovable: Bool
    public let recommendedMaxWorkingSetSize: UInt64
    public let supportsRaytracing: Bool
    public let vendorName: String
}

public enum QmvGpuVendor: UInt32 {
    case nvidia = 0x10DE
    case amd = 0x1002
    case intel = 0x8086
    case apple = 0x106B
    case unknown = 0x0000

    public var displayName: String {
        switch self {
        case .nvidia: return "NVIDIA"
        case .amd: return "AMD"
        case .intel: return "Intel"
        case .apple: return "Apple"
        case .unknown: return "Unknown"
        }
    }
}

public final class QmvGpuEnumerator {

    public static func enumerateAll() -> [QmvGpuInfo] {
        guard let devices = MTLCopyAllDevices() as [MTLDevice]? else {
            return []
        }

        return devices.map { device in
            buildInfo(from: device)
        }
    }

    public static func systemDefault() -> QmvGpuInfo? {
        guard let device = MTLCreateSystemDefaultDevice() else {
            return nil
        }
        return buildInfo(from: device)
    }

    public static func selectBestDevice(preferDiscrete: Bool) -> MTLDevice? {
        let devices = MTLCopyAllDevices()
        if devices.isEmpty {
            return nil
        }

        if preferDiscrete {
            let discrete = devices.filter { !$0.isLowPower && !$0.isRemovable }
            if let best = discrete.max(by: { $0.recommendedMaxWorkingSetSize < $1.recommendedMaxWorkingSetSize }) {
                return best
            }
        }

        return devices.max(by: { $0.recommendedMaxWorkingSetSize < $1.recommendedMaxWorkingSetSize })
    }

    private static func buildInfo(from device: MTLDevice) -> QmvGpuInfo {
        let vendorID = extractVendorID(from: device)
        let vendor = QmvGpuVendor(rawValue: vendorID) ?? .unknown

        return QmvGpuInfo(
            name: device.name,
            vendorID: vendorID,
            deviceID: 0,
            isLowPower: device.isLowPower,
            isRemovable: device.isRemovable,
            recommendedMaxWorkingSetSize: device.recommendedMaxWorkingSetSize,
            supportsRaytracing: device.supportsRaytracing,
            vendorName: vendor.displayName
        )
    }

    private static func extractVendorID(from device: MTLDevice) -> UInt32 {
        let name = device.name.lowercased()
        if name.contains("nvidia") || name.contains("geforce") || name.contains("quadro") || name.contains("rtx") || name.contains("gtx") {
            return QmvGpuVendor.nvidia.rawValue
        }
        if name.contains("amd") || name.contains("radeon") {
            return QmvGpuVendor.amd.rawValue
        }
        if name.contains("intel") || name.contains("iris") || name.contains("uhd") {
            return QmvGpuVendor.intel.rawValue
        }
        if name.contains("apple") || name.contains("m1") || name.contains("m2") || name.contains("m3") || name.contains("m4") {
            return QmvGpuVendor.apple.rawValue
        }
        return QmvGpuVendor.unknown.rawValue
    }
}

@_cdecl("qmv_swift_enumerate_gpu_count")
public func qmv_swift_enumerate_gpu_count() -> Int32 {
    return Int32(QmvGpuEnumerator.enumerateAll().count)
}

@_cdecl("qmv_swift_get_default_gpu_vendor")
public func qmv_swift_get_default_gpu_vendor() -> UInt32 {
    guard let info = QmvGpuEnumerator.systemDefault() else {
        return QmvGpuVendor.unknown.rawValue
    }
    return info.vendorID
}

@_cdecl("qmv_swift_get_default_gpu_working_set")
public func qmv_swift_get_default_gpu_working_set() -> UInt64 {
    guard let info = QmvGpuEnumerator.systemDefault() else {
        return 0
    }
    return info.recommendedMaxWorkingSetSize
}
