import Cocoa
import WebKit

final class AppDelegate: NSObject, NSApplicationDelegate, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler, WKDownloadDelegate {
    var window: NSWindow!
    var webView: WKWebView!
    var loading: NSTextField!
    var baseURL: URL?
    var ready = false
    // Native text follows the page: system language until the page reports its own setting.
    var turkish = (Locale.preferredLanguages.first ?? "").hasPrefix("tr")

    func L(_ english: String, _ turkishText: String) -> String { turkish ? turkishText : english }

    func applicationDidFinishLaunching(_ notification: Notification) {
        makeMenu()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1320, height: 860),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                          backing: .buffered, defer: false)
        window.title = "IndyMAT"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.minSize = NSSize(width: 780, height: 540)
        window.isReleasedWhenClosed = false
        window.center()
        window.setFrameAutosaveName("IndyMAT.MainWindow")
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .default()
        // The toolbar doubles as the title bar: the page marks itself as native
        // and asks the window to start a drag or zoom from empty toolbar space.
        let script = """
        document.documentElement.classList.add('native-window');
        addEventListener('mousedown', function (e) {
          if (e.button !== 0 || !e.target.closest('.toolbar') || e.target.closest('button,summary,input,a,details,select')) return;
          webkit.messageHandlers.titlebar.postMessage(e.detail);
        }, true);
        """
        config.userContentController.addUserScript(WKUserScript(source: script, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        config.userContentController.add(self, name: "titlebar")
        let languageScript = """
        (function () {
          const report = () => webkit.messageHandlers.language.postMessage(document.documentElement.lang || '');
          new MutationObserver(report).observe(document.documentElement, { attributes: true, attributeFilter: ['lang'] });
        })();
        """
        config.userContentController.addUserScript(WKUserScript(source: languageScript, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        config.userContentController.add(self, name: "language")
        webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.autoresizingMask = [.width, .height]
        webView.frame = window.contentView!.bounds
        window.contentView!.addSubview(webView)
        loading = NSTextField(wrappingLabelWithString: L("Starting IndyMAT…", "IndyMAT başlatılıyor…"))
        loading.alignment = .center
        loading.font = .systemFont(ofSize: 17)
        loading.frame = NSRect(x: 30, y: 380, width: 1260, height: 80)
        loading.autoresizingMask = [.width, .minYMargin, .maxYMargin]
        window.contentView!.addSubview(loading)
        for (name, on) in [(NSWindow.willEnterFullScreenNotification, true), (NSWindow.didExitFullScreenNotification, false)] {
            NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
                self?.webView.evaluateJavaScript("document.documentElement.classList.toggle('native-fullscreen', \(on))")
            }
        }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        launchServer()
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        if message.name == "language" {
            guard let value = message.body as? String, value == "tr" || value == "en", (value == "tr") != turkish else { return }
            turkish = value == "tr"
            makeMenu()
            return
        }
        guard message.name == "titlebar", let event = NSApp.currentEvent else { return }
        if (message.body as? Int) == 2 {
            window.performZoom(nil)
        } else {
            window.performDrag(with: event)
        }
    }

    func makeMenu() {
        let bar = NSMenu()
        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: L("About IndyMAT", "IndyMAT Hakkında"), action: #selector(about), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: L("Hide IndyMAT", "IndyMAT’ı Gizle"), action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(withTitle: L("Quit IndyMAT", "IndyMAT’tan Çık"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        bar.addItem(appItem)
        let editItem = NSMenuItem(title: L("Edit", "Düzen"), action: nil, keyEquivalent: "")
        let edit = NSMenu(title: L("Edit", "Düzen"))
        for (title, selector, key) in [(L("Undo", "Geri Al"), "undo:", "z"), (L("Cut", "Kes"), "cut:", "x"), (L("Copy", "Kopyala"), "copy:", "c"), (L("Paste", "Yapıştır"), "paste:", "v"), (L("Select All", "Tümünü Seç"), "selectAll:", "a")] {
            edit.addItem(withTitle: title, action: Selector(selector), keyEquivalent: key)
        }
        let redo = NSMenuItem(title: L("Redo", "Yinele"), action: Selector("redo:"), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        edit.insertItem(redo, at: 1)
        editItem.submenu = edit
        bar.addItem(editItem)
        let viewItem = NSMenuItem(title: L("View", "Görünüm"), action: nil, keyEquivalent: "")
        let view = NSMenu(title: L("View", "Görünüm"))
        view.addItem(withTitle: L("Reload", "Yeniden Yükle"), action: #selector(reload), keyEquivalent: "r")
        view.addItem(withTitle: L("Minimize", "Simge Durumuna Küçült"), action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        viewItem.submenu = view
        bar.addItem(viewItem)
        NSApp.mainMenu = bar
    }

    @objc func about() {
        let alert = NSAlert()
        alert.messageText = "IndyMAT"
        alert.informativeText = L("Local scientific workspace · GNU Octave\nClosing the window does not stop the computation session.", "Yerel bilimsel çalışma alanı · GNU Octave\nPencereyi kapatmak hesaplama oturumunu durdurmaz.")
        alert.runModal()
    }

    @objc func reload() {
        if ready { webView.reload() } else { launchServer() }
    }

    func launchServer() {
        loading.isHidden = false
        loading.stringValue = L("Starting IndyMAT…", "IndyMAT başlatılıyor…")
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let resource = Bundle.main.resourceURL!
                let config = try JSONSerialization.jsonObject(with: Data(contentsOf: resource.appendingPathComponent("launcher.json"))) as! [String: String]
                let process = Process()
                process.executableURL = URL(fileURLWithPath: config["python"]!)
                process.arguments = [resource.appendingPathComponent("launch.py").path, "--print-url"]
                let output = Pipe(), errors = Pipe()
                process.standardOutput = output
                process.standardError = errors
                try process.run()
                process.waitUntilExit()
                let text = String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                guard process.terminationStatus == 0, let url = URL(string: text), url.scheme == "http", url.host == "127.0.0.1", url.fragment != nil else {
                    let error = String(data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
                    throw NSError(domain: "IndyMAT", code: 1, userInfo: [NSLocalizedDescriptionKey: error.isEmpty ? self.L("The local server could not be started.", "Yerel sunucu başlatılamadı.") : error])
                }
                DispatchQueue.main.async {
                    self.baseURL = url
                    self.webView.load(URLRequest(url: url))
                }
            } catch {
                DispatchQueue.main.async { self.showFailure(error.localizedDescription) }
            }
        }
    }

    func showFailure(_ message: String) {
        ready = false
        loading.isHidden = false
        loading.stringValue = L("IndyMAT could not be opened.\n\(message)\nTry again with View → Reload.", "IndyMAT açılamadı.\n\(message)\nGörünüm → Yeniden Yükle ile tekrar deneyin.")
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        ready = true
        loading.isHidden = true
        window.makeFirstResponder(webView)
    }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        showFailure(L("The workspace could not be loaded. Check the local server connection.", "Çalışma alanı yüklenemedi. Yerel sunucu bağlantısını kontrol edin."))
    }
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        showFailure(L("The display process closed. The computation engine keeps running separately.", "Görüntüleme süreci kapandı. Hesaplama motoru ayrı çalışmaya devam eder."))
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        return true
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func webView(_ webView: WKWebView, runJavaScriptAlertPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping () -> Void) {
        let alert = NSAlert(); alert.messageText = "IndyMAT"; alert.informativeText = message
        alert.beginSheetModal(for: window) { _ in completionHandler() }
    }
    func webView(_ webView: WKWebView, runJavaScriptConfirmPanelWithMessage message: String, initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping (Bool) -> Void) {
        let alert = NSAlert(); alert.messageText = "IndyMAT"; alert.informativeText = message
        alert.addButton(withTitle: L("Continue", "Devam Et")); alert.addButton(withTitle: L("Cancel", "Vazgeç"))
        alert.beginSheetModal(for: window) { result in completionHandler(result == .alertFirstButtonReturn) }
    }
    func webView(_ webView: WKWebView, runOpenPanelWith parameters: WKOpenPanelParameters, initiatedByFrame frame: WKFrameInfo, completionHandler: @escaping ([URL]?) -> Void) {
        let panel = NSOpenPanel(); panel.allowsMultipleSelection = parameters.allowsMultipleSelection
        panel.canChooseDirectories = false
        panel.beginSheetModal(for: window) { response in completionHandler(response == .OK ? panel.urls : nil) }
    }
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = navigationAction.request.url else { decisionHandler(.cancel); return }
        if navigationAction.shouldPerformDownload || url.scheme == "blob" { decisionHandler(.download); return }
        if url.scheme == "about" || (url.scheme == baseURL?.scheme && url.host == baseURL?.host && url.port == baseURL?.port) {
            decisionHandler(.allow)
        } else {
            if navigationAction.navigationType == .linkActivated { NSWorkspace.shared.open(url) }
            decisionHandler(.cancel)
        }
    }
    func webView(_ webView: WKWebView, navigationAction: WKNavigationAction, didBecome download: WKDownload) { download.delegate = self }
    func download(_ download: WKDownload, decideDestinationUsing response: URLResponse, suggestedFilename: String, completionHandler: @escaping (URL?) -> Void) {
        let panel = NSSavePanel(); panel.nameFieldStringValue = suggestedFilename
        panel.beginSheetModal(for: window) { result in completionHandler(result == .OK ? panel.url : nil) }
    }
    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        let alert = NSAlert(); alert.messageText = L("The file could not be downloaded", "Dosya indirilemedi"); alert.informativeText = error.localizedDescription
        alert.beginSheetModal(for: window)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
