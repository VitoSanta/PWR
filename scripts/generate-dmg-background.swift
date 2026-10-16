import AppKit

let output = CommandLine.arguments.dropFirst().first ?? "apps/desktop/src-tauri/icons/dmg-background.png"
let size = NSSize(width: 660, height: 400)
let image = NSBitmapImageRep(
    bitmapDataPlanes: nil,
    pixelsWide: 1320,
    pixelsHigh: 800,
    bitsPerSample: 8,
    samplesPerPixel: 4,
    hasAlpha: true,
    isPlanar: false,
    colorSpaceName: .deviceRGB,
    bytesPerRow: 0,
    bitsPerPixel: 0
)!
image.size = size

let graphics = NSGraphicsContext(bitmapImageRep: image)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphics
graphics.imageInterpolation = .high

let bounds = NSRect(origin: .zero, size: size)
NSGradient(colors: [
    NSColor(calibratedRed: 0.035, green: 0.055, blue: 0.095, alpha: 1),
    NSColor(calibratedRed: 0.055, green: 0.085, blue: 0.145, alpha: 1),
])!.draw(in: bounds, angle: 0)

let grid = NSBezierPath()
for x in stride(from: 24, through: 636, by: 32) {
    grid.move(to: NSPoint(x: x, y: 18))
    grid.line(to: NSPoint(x: x, y: 382))
}
for y in stride(from: 20, through: 380, by: 32) {
    grid.move(to: NSPoint(x: 20, y: y))
    grid.line(to: NSPoint(x: 640, y: y))
}
NSColor(calibratedWhite: 1, alpha: 0.035).setStroke()
grid.lineWidth = 0.5
grid.stroke()

let frame = NSBezierPath(roundedRect: bounds.insetBy(dx: 16, dy: 16), xRadius: 14, yRadius: 14)
NSColor(calibratedRed: 0.45, green: 0.62, blue: 0.9, alpha: 0.18).setStroke()
frame.lineWidth = 1
frame.stroke()

func drawText(_ text: String, x: CGFloat, y: CGFloat, font: NSFont, color: NSColor) {
    (text as NSString).draw(at: NSPoint(x: x, y: y), withAttributes: [
        .font: font,
        .foregroundColor: color,
    ])
}

drawText("PWR", x: 46, y: 327, font: .systemFont(ofSize: 30, weight: .semibold), color: .white)
drawText("YOUR LOCAL AI WORKSPACE", x: 130, y: 337, font: .monospacedSystemFont(ofSize: 9, weight: .medium), color: NSColor(calibratedRed: 0.57, green: 0.72, blue: 0.98, alpha: 1))
drawText("A private workspace for building with local AI.", x: 46, y: 306, font: .systemFont(ofSize: 12, weight: .regular), color: NSColor(calibratedWhite: 0.78, alpha: 1))

let arrow = NSBezierPath()
arrow.move(to: NSPoint(x: 230, y: 202))
arrow.line(to: NSPoint(x: 308, y: 202))
arrow.move(to: NSPoint(x: 352, y: 202))
arrow.line(to: NSPoint(x: 430, y: 202))
arrow.move(to: NSPoint(x: 337, y: 213))
arrow.line(to: NSPoint(x: 348, y: 202))
arrow.line(to: NSPoint(x: 337, y: 191))
NSColor(calibratedRed: 0.42, green: 0.65, blue: 1, alpha: 0.82).setStroke()
arrow.lineWidth = 1.5
arrow.lineCapStyle = .round
arrow.lineJoinStyle = .round
arrow.stroke()

let instruction = "DRAG PWR TO APPLICATIONS"
let instructionFont = NSFont.monospacedSystemFont(ofSize: 11, weight: .semibold)
let instructionWidth = (instruction as NSString).size(withAttributes: [.font: instructionFont]).width
drawText(instruction, x: (size.width - instructionWidth) / 2, y: 55, font: instructionFont, color: NSColor(calibratedRed: 0.73, green: 0.83, blue: 1, alpha: 1))
let note = "Models and data stay on this Mac."
let noteFont = NSFont.systemFont(ofSize: 10, weight: .regular)
let noteWidth = (note as NSString).size(withAttributes: [.font: noteFont]).width
drawText(note, x: (size.width - noteWidth) / 2, y: 36, font: noteFont, color: NSColor(calibratedWhite: 0.64, alpha: 1))

NSGraphicsContext.restoreGraphicsState()
let data = image.representation(using: .png, properties: [:])!
try data.write(to: URL(fileURLWithPath: output), options: .atomic)