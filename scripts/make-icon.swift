// Draws the macutil app icon from scratch (no external artwork) and writes
// packaging/Macutil.icns plus docs/icon.png. Run via `scripts/make-icon.sh`.
//
// The .icns artwork is full-bleed: macOS 26+ masks it into the system icon shape itself
// (an icon with its own rounded tile and margin gets shrunk onto a gray plate instead).
// The README preview has the rounded shape drawn in, since nothing masks it there.
//
//   swift scripts/make-icon.swift <output-dir-for-iconset> <preview.png>
import AppKit

/// A three-row menu, first row selected, on a dark tile. `rounded` draws the tile as a squircle with
/// the standard margin (for previews); otherwise the artwork fills the canvas (for the .icns).
func drawIcon(in ctx: CGContext, size: CGFloat, rounded: Bool) {
    ctx.scaleBy(x: size / 1024, y: size / 1024) // laid out on Apple's 1024pt icon grid

    if rounded {
        let tile = squircle(in: CGRect(x: 100, y: 100, width: 824, height: 824), exponent: 5)
        ctx.addPath(tile)
        ctx.setFillColor(CGColor(srgbRed: 0.102, green: 0.106, blue: 0.149, alpha: 1))
        ctx.fillPath()
        // Hairline edge so the tile reads on dark backgrounds too.
        ctx.saveGState()
        ctx.addPath(tile)
        ctx.setStrokeColor(CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 0.08))
        ctx.setLineWidth(4)
        ctx.strokePath()
        ctx.restoreGState()
        ctx.addPath(tile)
        ctx.clip()
        // Fit the full-bleed artwork into the tile.
        ctx.translateBy(x: 100, y: 100)
        ctx.scaleBy(x: 824 / 1024, y: 824 / 1024)
    }
    drawArtwork(in: ctx)
}

/// The full-bleed 1024x1024 artwork: flat colours only (Tokyo Night), no gradients or glow.
func drawArtwork(in ctx: CGContext) {
    ctx.setFillColor(CGColor(srgbRed: 0.102, green: 0.106, blue: 0.149, alpha: 1)) // #1a1b26
    ctx.fill(CGRect(x: 0, y: 0, width: 1024, height: 1024))

    // A menu (wifi · bluetooth · tailscale): the selected row as an accent block, the
    // others as text lines in the foreground colour. Same 72pt weight as the macofi glyph.
    ctx.setFillColor(CGColor(srgbRed: 0.478, green: 0.635, blue: 0.969, alpha: 1)) // #7aa2f7
    ctx.fill(CGRect(x: 292, y: 624, width: 440, height: 72))
    ctx.setFillColor(CGColor(srgbRed: 0.753, green: 0.792, blue: 0.961, alpha: 1)) // #c0caf5
    ctx.fill(CGRect(x: 292, y: 476, width: 340, height: 72))
    ctx.fill(CGRect(x: 292, y: 328, width: 260, height: 72))
}

/// |x|^n + |y|^n = 1, the continuous-curvature shape macOS uses for app icons.
func squircle(in rect: CGRect, exponent n: CGFloat) -> CGPath {
    let path = CGMutablePath()
    let steps = 720
    for i in 0...steps {
        let t = CGFloat(i) / CGFloat(steps) * 2 * .pi
        let c = cos(t), sn = sin(t)
        let x = pow(abs(c), 2 / n) * (c < 0 ? -1 : 1)
        let y = pow(abs(sn), 2 / n) * (sn < 0 ? -1 : 1)
        let p = CGPoint(x: rect.midX + x * rect.width / 2, y: rect.midY + y * rect.height / 2)
        i == 0 ? path.move(to: p) : path.addLine(to: p)
    }
    path.closeSubpath()
    return path
}

func png(size: Int, rounded: Bool = false) -> Data {
    let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    drawIcon(in: ctx, size: CGFloat(size), rounded: rounded)
    return NSBitmapImageRep(cgImage: ctx.makeImage()!).representation(using: .png, properties: [:])!
}

let args = CommandLine.arguments
guard args.count == 3 else { print("usage: make-icon.swift <iconset-dir> <preview.png>"); exit(2) }
let iconset = URL(fileURLWithPath: args[1])
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try png(size: base).write(to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    try png(size: base * 2).write(to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
try png(size: 512, rounded: true).write(to: URL(fileURLWithPath: args[2]))
