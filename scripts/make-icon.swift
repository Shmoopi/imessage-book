#!/usr/bin/env swift
//
// Draws the imessage-book app icon and writes an .iconset worth of PNGs.
//
// Usage: swift scripts/make-icon.swift <output-iconset-dir>
//
// The design: an iMessage-blue rounded tile, a white chat bubble with message lines, and
// a red bookmark ribbon — "a message that's also a book."
//
import AppKit
import ImageIO
import UniformTypeIdentifiers

func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(red: r / 255, green: g / 255, blue: b / 255, alpha: a)
}

func roundedRect(_ r: CGRect, _ radius: CGFloat) -> CGPath {
    CGPath(roundedRect: r, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

func drawIcon(size: Int) -> CGImage {
    let s = CGFloat(size)
    let space = CGColorSpaceCreateDeviceRGB()
    let ctx = CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    ctx.setAllowsAntialiasing(true)

    // Rounded background tile with the standard macOS margin + corner ratio.
    let margin = s * 0.085
    let tile = CGRect(x: margin, y: margin, width: s - 2 * margin, height: s - 2 * margin)
    let tilePath = roundedRect(tile, tile.width * 0.225)

    ctx.saveGState()
    ctx.addPath(tilePath)
    ctx.clip()
    let bg = CGGradient(colorsSpace: space,
                        colors: [rgb(56, 161, 255), rgb(10, 102, 240)] as CFArray,
                        locations: [0, 1])!
    ctx.drawLinearGradient(bg, start: CGPoint(x: 0, y: tile.maxY),
                           end: CGPoint(x: 0, y: tile.minY), options: [])
    // Soft top sheen for depth.
    let sheen = CGGradient(colorsSpace: space,
                           colors: [rgb(255, 255, 255, 0.18), rgb(255, 255, 255, 0)] as CFArray,
                           locations: [0, 1])!
    ctx.drawLinearGradient(sheen, start: CGPoint(x: 0, y: tile.maxY),
                           end: CGPoint(x: 0, y: tile.midY), options: [])
    ctx.restoreGState()

    // White chat bubble with a tail at the lower-left.
    let bubble = CGRect(x: s * 0.255, y: s * 0.40, width: s * 0.49, height: s * 0.345)
    let bubblePath = CGMutablePath()
    bubblePath.addPath(roundedRect(bubble, s * 0.075))
    let tail = CGMutablePath()
    tail.move(to: CGPoint(x: s * 0.36, y: bubble.minY + s * 0.004))
    tail.addLine(to: CGPoint(x: s * 0.305, y: s * 0.315))
    tail.addLine(to: CGPoint(x: s * 0.47, y: bubble.minY + s * 0.004))
    tail.closeSubpath()

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -s * 0.006), blur: s * 0.02, color: rgb(0, 0, 0, 0.20))
    ctx.setFillColor(rgb(255, 255, 255))
    ctx.addPath(bubblePath)
    ctx.addPath(tail)
    ctx.fillPath()
    ctx.restoreGState()

    // Message lines inside the bubble.
    func line(_ x0: CGFloat, _ x1: CGFloat, _ y: CGFloat) {
        ctx.addPath(roundedRect(CGRect(x: s * x0, y: s * y, width: s * (x1 - x0), height: s * 0.026),
                                s * 0.013))
        ctx.fillPath()
    }
    ctx.setFillColor(rgb(169, 205, 255))
    line(0.315, 0.60, 0.63)
    line(0.315, 0.55, 0.565)
    line(0.315, 0.50, 0.50)

    // Red bookmark ribbon poking out of the top-right, with a V-notch at the bottom.
    let rx0 = s * 0.645, rx1 = s * 0.70
    let rTop = s * 0.80, rBot = s * 0.585, notch = s * 0.022
    let ribbon = CGMutablePath()
    ribbon.move(to: CGPoint(x: rx0, y: rTop))
    ribbon.addLine(to: CGPoint(x: rx1, y: rTop))
    ribbon.addLine(to: CGPoint(x: rx1, y: rBot))
    ribbon.addLine(to: CGPoint(x: (rx0 + rx1) / 2, y: rBot + notch))
    ribbon.addLine(to: CGPoint(x: rx0, y: rBot))
    ribbon.closeSubpath()

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -s * 0.004), blur: s * 0.012, color: rgb(0, 0, 0, 0.18))
    ctx.setFillColor(rgb(255, 78, 90))
    ctx.addPath(ribbon)
    ctx.fillPath()
    ctx.restoreGState()

    return ctx.makeImage()!
}

func writePNG(_ image: CGImage, to url: URL) {
    let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
}

// --- main ---
let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write(Data("usage: make-icon.swift <iconset-dir>\n".utf8))
    exit(2)
}
let outDir = URL(fileURLWithPath: args[1])
try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

let specs: [(String, Int)] = [
    ("icon_16x16", 16), ("icon_16x16@2x", 32),
    ("icon_32x32", 32), ("icon_32x32@2x", 64),
    ("icon_128x128", 128), ("icon_128x128@2x", 256),
    ("icon_256x256", 256), ("icon_256x256@2x", 512),
    ("icon_512x512", 512), ("icon_512x512@2x", 1024),
]
for (name, size) in specs {
    writePNG(drawIcon(size: size), to: outDir.appendingPathComponent("\(name).png"))
}
print("Wrote \(specs.count) icon sizes to \(outDir.path)")
