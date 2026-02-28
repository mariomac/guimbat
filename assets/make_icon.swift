// Generates a 1024×1024 calculator app icon PNG using Core Graphics.
// License of the resulting artwork: CC0 (original geometry).
// Run: swift make_icon.swift
import AppKit
import CoreGraphics

let size = 1024
let cs = CGColorSpace(name: CGColorSpace.sRGB)!

guard let ctx = CGContext(
    data: nil, width: size, height: size, bitsPerComponent: 8,
    bytesPerRow: size * 4, space: cs,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { fatalError("context") }

let s = CGFloat(size)

// ── background: rounded rect, dark blue-grey ─────────────────────────────────
ctx.setFillColor(CGColor(srgbRed: 0.13, green: 0.16, blue: 0.22, alpha: 1))
let bgPath = CGMutablePath()
bgPath.addRoundedRect(in: CGRect(x: 0, y: 0, width: s, height: s),
                      cornerWidth: s * 0.22, cornerHeight: s * 0.22)
ctx.addPath(bgPath)
ctx.fillPath()

// ── helper: filled rounded rect ───────────────────────────────────────────────
func fillRR(_ rect: CGRect, radius: CGFloat, color: CGColor) {
    ctx.setFillColor(color)
    let p = CGMutablePath()
    p.addRoundedRect(in: rect, cornerWidth: radius, cornerHeight: radius)
    ctx.addPath(p)
    ctx.fillPath()
}

// ── display area ─────────────────────────────────────────────────────────────
let displayGreen = CGColor(srgbRed: 0.31, green: 0.78, blue: 0.47, alpha: 1)
let displayRect = CGRect(x: s*0.14, y: s*0.60, width: s*0.72, height: s*0.24)
fillRR(displayRect, radius: s*0.04, color: CGColor(srgbRed: 0.08, green: 0.11, blue: 0.16, alpha: 1))

// draw "= 42" text on display
let paraStyle = NSMutableParagraphStyle()
paraStyle.alignment = .right
let attrs: [NSAttributedString.Key: Any] = [
    .font: NSFont.monospacedDigitSystemFont(ofSize: s * 0.14, weight: .light),
    .foregroundColor: NSColor(cgColor: displayGreen)!,
    .paragraphStyle: paraStyle,
]
let nsCtx = NSGraphicsContext(cgContext: ctx, flipped: false)
NSGraphicsContext.current = nsCtx
let textRect = CGRect(x: s*0.17, y: s*0.63, width: s*0.66, height: s*0.16)
"42".draw(in: textRect, withAttributes: attrs)

// ── button grid: 4 columns × 4 rows ──────────────────────────────────────────
let cols = 4, rows = 4
let gridX = s * 0.10, gridY = s * 0.08
let gridW = s * 0.80, gridH = s * 0.46
let gap   = s * 0.04
let btnW  = (gridW - gap * CGFloat(cols - 1)) / CGFloat(cols)
let btnH  = (gridH - gap * CGFloat(rows - 1)) / CGFloat(rows)

// colour per column: operator (rightmost) gets accent, rest get dark tile
let tileColor   = CGColor(srgbRed: 0.20, green: 0.24, blue: 0.32, alpha: 1)
let opColor     = CGColor(srgbRed: 0.99, green: 0.47, blue: 0.24, alpha: 1) // orange
let equalColor  = CGColor(srgbRed: 0.99, green: 0.47, blue: 0.24, alpha: 1)

for row in 0..<rows {
    for col in 0..<cols {
        let x = gridX + CGFloat(col) * (btnW + gap)
        let y = gridY + CGFloat(row) * (btnH + gap)
        let isOp    = col == 3
        let isEqual = row == 0 && col == 3
        let color   = isOp ? (isEqual ? equalColor : opColor) : tileColor
        fillRR(CGRect(x: x, y: y, width: btnW, height: btnH),
               radius: s * 0.03, color: color)
    }
}

// ── export PNG ────────────────────────────────────────────────────────────────
guard let img = ctx.makeImage() else { fatalError("image") }
let nsImg = NSImage(cgImage: img, size: NSSize(width: size, height: size))
guard let tiff = nsImg.tiffRepresentation,
      let rep  = NSBitmapImageRep(data: tiff),
      let png  = rep.representation(using: .png, properties: [:])
else { fatalError("png") }

let out = URL(fileURLWithPath: "icon_1024.png")
try! png.write(to: out)
print("Written: \(out.path)")
