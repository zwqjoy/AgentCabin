import Foundation
import CoreGraphics
import ImageIO
import Vision

// Fast macOS Apple Vision OCR runner.
// Reads base64 image data from stdin or argument, recognizes text via VNRecognizeTextRequest,
// and prints JSON output: [{ "text": String, "confidence": Double, "rect": { "x": Double, "y": Double, "width": Double, "height": Double } }]

func exitWithError(_ message: String, code: Int32 = 1) -> Never {
    let errorJson = ["error": message]
    if let data = try? JSONSerialization.data(withJSONObject: errorJson),
       let str = String(data: data, encoding: .utf8) {
        fputs("\(str)\n", stderr)
    } else {
        fputs("Error: \(message)\n", stderr)
    }
    exit(code)
}

func main() {
    var rawInput: String = ""
    if CommandLine.arguments.count > 1 {
        let arg = CommandLine.arguments[1]
        if FileManager.default.fileExists(atPath: arg) {
            if let fileData = try? Data(contentsOf: URL(fileURLWithPath: arg)) {
                rawInput = fileData.base64EncodedString()
            } else {
                exitWithError("Failed to read image file at \(arg)")
            }
        } else {
            rawInput = arg
        }
    } else {
        let stdinData = FileHandle.standardInput.readDataToEndOfFile()
        rawInput = String(data: stdinData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    if rawInput.isEmpty {
        print("[]")
        exit(0)
    }

    // Strip optional data:image/...;base64, prefix
    if let commaIndex = rawInput.firstIndex(of: ",") {
        let prefix = rawInput[..<commaIndex]
        if prefix.contains("base64") {
            rawInput = String(rawInput[rawInput.index(after: commaIndex)...])
        }
    }

    guard let imageData = Data(base64Encoded: rawInput, options: [.ignoreUnknownCharacters]) else {
        exitWithError("Invalid base64 payload")
    }

    guard let dataProvider = CGDataProvider(data: imageData as CFData),
          let cgImage = CGImage(pngDataProviderSource: dataProvider, decode: nil, shouldInterpolate: false, intent: .defaultIntent) ??
                        CGImage(jpegDataProviderSource: dataProvider, decode: nil, shouldInterpolate: false, intent: .defaultIntent) ??
                        (CGImageSourceCreateWithData(imageData as CFData, nil).flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }) else {
        exitWithError("Unable to decode image from provided data")
    }

    let imageWidth = Double(cgImage.width)
    let imageHeight = Double(cgImage.height)

    let semaphore = DispatchSemaphore(value: 0)
    var recognizedBoxes: [[String: Any]] = []
    var requestError: Error?

    let request = VNRecognizeTextRequest { req, err in
        defer { semaphore.signal() }
        if let err = err {
            requestError = err
            return
        }

        let observations = (req.results as? [VNRecognizedTextObservation]) ?? []
        for obs in observations {
            guard let topCandidate = obs.topCandidates(1).first else { continue }
            let box = obs.boundingBox
            let x: Double = box.origin.x * imageWidth
            let y: Double = (1.0 - box.origin.y - box.height) * imageHeight
            let w: Double = box.width * imageWidth
            let h: Double = box.height * imageHeight

            let rectDict: [String: Double] = [
                "x": round(x * 10) / 10,
                "y": round(y * 10) / 10,
                "width": round(w * 10) / 10,
                "height": round(h * 10) / 10
            ]
            let item: [String: Any] = [
                "text": topCandidate.string,
                "confidence": Double(topCandidate.confidence),
                "rect": rectDict
            ]
            recognizedBoxes.append(item)
        }
    }

    request.recognitionLevel = VNRequestTextRecognitionLevel.accurate
    request.usesLanguageCorrection = false

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    do {
        try handler.perform([request])
    } catch {
        exitWithError("Vision request handler failed: \(error.localizedDescription)")
    }

    if semaphore.wait(timeout: .now() + .seconds(10)) == .timedOut {
        exitWithError("Vision text recognition timed out")
    }

    if let err = requestError {
        exitWithError("Text recognition failed: \(err.localizedDescription)")
    }

    if let outputData = try? JSONSerialization.data(withJSONObject: recognizedBoxes, options: []),
       let outputStr = String(data: outputData, encoding: .utf8) {
        print(outputStr)
    } else {
        print("[]")
    }
}

main()
