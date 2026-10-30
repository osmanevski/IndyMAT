import Cocoa

// The launch screen over the web view: the header's logo lockup and colours, the brand kit's type.
struct EngineOption {
    let id: String
    let name: String
    let note: String
    let enabled: Bool
    let hint: String?
}

private enum Brand {
    static func color(light: UInt32, dark: UInt32) -> NSColor {
        NSColor(name: nil) { appearance in
            let value = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
            return NSColor(srgbRed: CGFloat(value >> 16 & 255) / 255, green: CGFloat(value >> 8 & 255) / 255, blue: CGFloat(value & 255) / 255, alpha: 1)
        }
    }
    // The app's own tokens in static/style.css; tests/test_launch_engine.py pins them.
    static let background = color(light: 0xC2D5E6, dark: 0x080B21)  // --chrome
    static let text = color(light: 0x111A32, dark: 0xE0E0E0)        // --text
    static let secondary = color(light: 0x35495A, dark: 0x93A4B0)   // --muted
    static let line = color(light: 0x92AEC6, dark: 0x35424C)        // --line

    static func resource(_ name: String) -> URL? {
        Bundle.main.resourceURL?.appendingPathComponent("splash").appendingPathComponent(name)
    }
    // Montserrat without installing it. Each weight comes in two files, and Turkish needs both:
    // latin-ext carries ş ğ İ. The system font stands in if the files are missing.
    static func font(_ size: CGFloat, weight: Int) -> NSFont {
        func descriptor(_ subset: String) -> CTFontDescriptor? {
            guard let url = resource("montserrat-\(subset)-\(weight)-normal.woff2"), let data = try? Data(contentsOf: url) else { return nil }
            return CTFontManagerCreateFontDescriptorFromData(data as CFData)
        }
        guard let latin = descriptor("latin"), let extended = descriptor("latin-ext") else {
            return .systemFont(ofSize: size, weight: weight == 600 ? .semibold : .regular)
        }
        let cascade = CTFontDescriptorCreateCopyWithAttributes(latin, [kCTFontCascadeListAttribute: [extended]] as CFDictionary)
        return CTFontCreateWithFontDescriptor(cascade, size, nil) as NSFont
    }
}

private func label(_ text: String, font: NSFont, color: NSColor) -> NSTextField {
    let field = NSTextField(wrappingLabelWithString: text)
    field.font = font
    field.textColor = color
    field.isSelectable = false
    field.alignment = .center
    return field
}

// One engine: a click starts it, and the selected one is what Return starts.
final class EngineRow: NSView {
    let option: EngineOption
    var onPick: (() -> Void)?
    var selected = false { didSet { needsDisplay = true } }
    private var hovered = false { didSet { needsDisplay = true } }

    init(_ option: EngineOption) {
        self.option = option
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        let name = label(option.name, font: Brand.font(15, weight: 600), color: option.enabled ? Brand.text : Brand.secondary)
        let note = label(option.note, font: Brand.font(13, weight: 400), color: Brand.secondary)
        for field in [name, note] { field.setContentHuggingPriority(.required, for: .horizontal) }
        let line = NSStackView(views: [name, note, NSView()])
        line.spacing = 10
        line.alignment = .firstBaseline
        let lines = NSStackView(views: [line])
        lines.orientation = .vertical
        lines.alignment = .leading
        lines.spacing = 6
        if let hint = option.hint {
            let field = label(hint, font: .monospacedSystemFont(ofSize: 12, weight: .regular), color: Brand.secondary)
            field.alignment = .left
            lines.addArrangedSubview(field)
        }
        lines.translatesAutoresizingMaskIntoConstraints = false
        addSubview(lines)
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 440),
            line.widthAnchor.constraint(equalTo: lines.widthAnchor),
            lines.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            lines.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -18),
            lines.topAnchor.constraint(equalTo: topAnchor, constant: 15),
            lines.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -15),
        ])
    }
    required init?(coder: NSCoder) { nil }

    override func draw(_ dirtyRect: NSRect) {
        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 10, yRadius: 10)
        path.lineWidth = selected ? 2 : 1
        (selected ? Brand.text : hovered && option.enabled ? Brand.secondary : Brand.line).setStroke()
        path.stroke()
    }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeInKeyWindow], owner: self))
    }
    override func mouseEntered(with event: NSEvent) { hovered = true }
    override func mouseExited(with event: NSEvent) { hovered = false }
    override func hitTest(_ point: NSPoint) -> NSView? { frame.contains(point) ? self : nil }
    override func mouseDown(with event: NSEvent) {}
    override func mouseUp(with event: NSEvent) {
        if option.enabled && bounds.contains(convert(event.locationInWindow, from: nil)) { onPick?() }
    }
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .button }
    override func accessibilityLabel() -> String? { [option.name, option.note, option.hint].compactMap { $0 }.joined(separator: ", ") }
    override func isAccessibilityEnabled() -> Bool { option.enabled }
    override func accessibilityPerformPress() -> Bool {
        if option.enabled { onPick?() }
        return option.enabled
    }
}

final class SplashView: NSView {
    private let logo = NSImageView()
    private let content = NSStackView()
    private let spinner = NSProgressIndicator()
    private var rows: [EngineRow] = []
    private var onChoice: ((String) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        logo.translatesAutoresizingMaskIntoConstraints = false
        logo.imageScaling = .scaleProportionallyUpOrDown
        NSLayoutConstraint.activate([logo.widthAnchor.constraint(equalToConstant: 360), logo.heightAnchor.constraint(equalToConstant: 93.5)])
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        let stack = NSStackView(views: [logo, content])
        stack.orientation = .vertical
        stack.spacing = 40
        content.orientation = .vertical
        content.spacing = 10
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        updateLogo()
    }
    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { false }
    override var acceptsFirstResponder: Bool { !rows.isEmpty }
    override var mouseDownCanMoveWindow: Bool { true }
    // Everything under the splash stays out of reach until it is dismissed.
    override func hitTest(_ point: NSPoint) -> NSView? { isHidden ? nil : super.hitTest(point) ?? self }
    override func draw(_ dirtyRect: NSRect) {
        Brand.background.setFill()
        dirtyRect.fill()
    }
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateLogo()
    }
    private func updateLogo() {
        let variant = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? "white" : "black"
        logo.image = Brand.resource("indymat-logo-horizontal-\(variant)-1024.png").flatMap(NSImage.init(contentsOf:))
    }

    private func setContent(_ views: [NSView]) {
        spinner.stopAnimation(nil)
        rows = []
        onChoice = nil
        content.arrangedSubviews.forEach { $0.removeFromSuperview() }
        views.forEach(content.addArrangedSubview)
        isHidden = false
        alphaValue = 1
    }

    func showProgress(_ text: String) {
        setContent(text.isEmpty ? [spinner] : [spinner, label(text, font: Brand.font(13, weight: 400), color: Brand.secondary)])
        spinner.startAnimation(nil)
    }

    func showMessage(_ text: String) {
        let field = label(text, font: Brand.font(14, weight: 400), color: Brand.text)
        field.preferredMaxLayoutWidth = 560
        setContent([field])
    }

    func showChoice(question: String, footnote: String, options: [EngineOption], selected: String, onChoice: @escaping (String) -> Void) {
        let title = label(question, font: Brand.font(20, weight: 600), color: Brand.text)
        let foot = label(footnote, font: Brand.font(12, weight: 400), color: Brand.secondary)
        foot.preferredMaxLayoutWidth = 440
        let choices = options.map(EngineRow.init)
        setContent([title] + choices + [foot])
        content.setCustomSpacing(18, after: title)
        content.setCustomSpacing(16, after: choices.last!)
        rows = choices
        self.onChoice = onChoice
        for row in rows {
            row.onPick = { [weak self, id = row.option.id] in self?.pick(id) }
            row.selected = row.option.id == selected && row.option.enabled
        }
        if !rows.contains(where: \.selected) { rows.first(where: \.option.enabled)?.selected = true }
        window?.makeFirstResponder(self)
    }

    private func pick(_ id: String) {
        let choice = onChoice
        onChoice = nil
        choice?(id)
    }

    override func keyDown(with event: NSEvent) {
        guard !rows.isEmpty else { return super.keyDown(with: event) }
        switch event.keyCode {
        case 36, 76:  // Return, Enter
            if let row = rows.first(where: \.selected) { pick(row.option.id) }
        case 125, 124, 48:  // down, right, Tab
            moveSelection(by: event.modifierFlags.contains(.shift) ? -1 : 1)
        case 126, 123:  // up, left
            moveSelection(by: -1)
        default:
            super.keyDown(with: event)
        }
    }

    private func moveSelection(by step: Int) {
        let enabled = rows.filter(\.option.enabled)
        guard let current = enabled.firstIndex(where: \.selected) else { return }
        let next = enabled[(current + step + enabled.count) % enabled.count]
        rows.forEach { $0.selected = $0 === next }
    }

    func dismiss() {
        guard !isHidden else { return }  // Every later page load ends here too.
        setContent([])
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.18
            animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            // A failure or a reload may have brought the splash back during the fade.
            guard let self = self else { return }
            if self.content.arrangedSubviews.isEmpty { self.isHidden = true } else { self.alphaValue = 1 }
        })
    }
}
