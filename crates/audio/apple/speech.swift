// boardcast-speech: Apple's sentence and live speech recognition for boardcast.
//
//   boardcast-speech serve <locale|auto>
//     Prints {"type":"ready","engine":"analyzer"|"legacy","locale":…} once it can recognize, or
//     {"type":"error","message":…} and exits. Then, for each sentence, reads a little-endian UInt32
//     sample count and that many 16 kHz mono float32 samples from stdin, and prints
//     {"type":"text","text":…} or {"type":"error","message":…}. A count of 0, or the end of stdin, ends it.
//     When ready includes streaming:true, UInt32.max switches to continuous raw PCM input until EOF.
//     After streaming readiness, partial/final events contain text and start/end seconds; EOF emits done.
//   boardcast-speech self-test
//     Checks the audio conversion, for the build.
//
// SpeechAnalyzer (macOS 26) is used when it supports the language and its model can be installed;
// otherwise SFSpeechRecognizer, on the device when it can be.

import AVFoundation
import Foundation
import Speech

private let outputLock = NSLock()

func emit(_ event: [String: Any]) {
    guard var data = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]) else { return }
    data.append(10)
    outputLock.lock()
    defer { outputLock.unlock() }
    FileHandle.standardOutput.write(data)
}

struct Failure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

let inputFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!

/// The samples as a buffer in the format they arrive in.
func buffer(_ samples: [Float]) -> AVAudioPCMBuffer {
    let buffer = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: AVAudioFrameCount(max(samples.count, 1)))!
    buffer.frameLength = AVAudioFrameCount(samples.count)
    samples.withUnsafeBufferPointer { source in
        if let base = source.baseAddress {
            buffer.floatChannelData![0].update(from: base, count: samples.count)
        }
    }
    return buffer
}

/// The samples in `format`, which the recognizer asks for.
func convert(_ samples: [Float], to format: AVAudioFormat) throws -> AVAudioPCMBuffer {
    let input = buffer(samples)
    if format == inputFormat { return input }
    guard let converter = AVAudioConverter(from: inputFormat, to: format) else {
        throw Failure(message: "Cannot convert audio to Apple's input format")
    }
    converter.primeMethod = .none
    let capacity = AVAudioFrameCount(ceil(Double(samples.count) * format.sampleRate / 16_000)) + 64
    let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity)!
    var supplied = false
    var error: NSError?
    let status = converter.convert(to: output, error: &error) { _, state in
        if supplied {
            state.pointee = .endOfStream
            return nil
        }
        supplied = true
        state.pointee = .haveData
        return input
    }
    if let error { throw error }
    if status == .error { throw Failure(message: "Audio conversion failed") }
    return output
}

// Pull-based input keeps file imports and a slow recognizer from accumulating unbounded audio.
final class PCMInput {
    let format: AVAudioFormat
    let converter: AVAudioConverter
    let read: () throws -> Data
    var pending = Data()
    var ended = false

    init(format: AVAudioFormat, read: @escaping () throws -> Data) throws {
        let source = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000,
                                   channels: 1, interleaved: false)!
        guard let converter = AVAudioConverter(from: source, to: format) else {
            throw Failure(message: "Cannot convert audio to Apple's input format")
        }
        converter.primeMethod = .none
        self.format = format
        self.converter = converter
        self.read = read
    }

    func next() throws -> AVAudioPCMBuffer? {
        while true {
            if !ended {
                let bytes = try read()
                ended = bytes.isEmpty
                pending.append(bytes)
            }
            if ended && pending.count % 4 != 0 {
                throw Failure(message: "Incomplete float32 audio sample")
            }
            let frames = pending.count / 4
            if frames == 0 && !ended { continue }
            let input = AVAudioPCMBuffer(pcmFormat: converter.inputFormat,
                                          frameCapacity: AVAudioFrameCount(max(1, frames)))!
            input.frameLength = AVAudioFrameCount(frames)
            if frames > 0 {
                _ = pending.copyBytes(to: UnsafeMutableRawBufferPointer(start: input.floatChannelData![0], count: frames * 4))
                pending.removeFirst(frames * 4)
                guard UnsafeBufferPointer(start: input.floatChannelData![0], count: frames).allSatisfy(\.isFinite) else {
                    throw Failure(message: "Invalid streaming samples")
                }
            }
            let capacity = AVAudioFrameCount(ceil(Double(max(frames, 1)) * format.sampleRate / 16_000)) + 64
            let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity)!
            var supplied = false
            var error: NSError?
            let status = converter.convert(to: output, error: &error) { _, state in
                if !supplied && frames > 0 {
                    supplied = true
                    state.pointee = .haveData
                    return input
                }
                state.pointee = self.ended ? .endOfStream : .noDataNow
                return nil
            }
            if let error { throw error }
            if status == .error { throw Failure(message: "Audio conversion failed") }
            if output.frameLength > 0 { return output }
            if status == .endOfStream { return nil }
        }
    }
}

/// Exactly `count` bytes from stdin, or nil at its end.
func readExactly(_ count: Int) throws -> Data? {
    var data = Data()
    while data.count < count {
        guard let chunk = try FileHandle.standardInput.read(upToCount: count - data.count), !chunk.isEmpty else {
            if data.isEmpty { return nil }
            throw Failure(message: "Truncated sentence input")
        }
        data.append(chunk)
    }
    return data
}

enum Input { case end, sentence([Float]), stream }

func nextInput() throws -> Input {
    guard let header = try readExactly(4) else { return .end }
    let count = Int(header.withUnsafeBytes { UInt32(littleEndian: $0.loadUnaligned(as: UInt32.self)) })
    if count == 0 { return .end }
    if count == Int(UInt32.max) { return .stream }
    guard count <= 16_000 * 30, let body = try readExactly(count * 4) else {
        throw Failure(message: "Invalid sentence length")
    }
    let samples = body.withUnsafeBytes { raw in
        (0..<count).map { Float(bitPattern: UInt32(littleEndian: raw.loadUnaligned(fromByteOffset: $0 * 4, as: UInt32.self))) }
    }
    guard samples.allSatisfy(\.isFinite) else { throw Failure(message: "Invalid sentence samples") }
    return .sentence(samples)
}

protocol Engine {
    var name: String { get }
    var locale: Locale { get }
    func recognize(_ samples: [Float]) async throws -> String
}

@available(macOS 26.0, *)
final class Analyzer: Engine {
    let name = "analyzer"
    let locale: Locale

    init(locale: Locale) {
        self.locale = locale
    }

    func module(live: Bool = false) -> SpeechTranscriber {
        SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: live ? [.volatileResults] : [], attributeOptions: live ? [.audioTimeRange] : [])
    }

    /// The language model is on the device, downloading it if the system allows.
    static func start(_ wanted: Locale) async throws -> Analyzer {
        guard SpeechTranscriber.isAvailable else { throw Failure(message: "SpeechAnalyzer is not available on this Mac") }
        guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: wanted) else {
            throw Failure(message: "SpeechAnalyzer does not support \(wanted.identifier)")
        }
        let analyzer = Analyzer(locale: locale)
        let module = analyzer.module()
        if await AssetInventory.status(forModules: [module]) != .installed {
            emit(["type": "preparing", "locale": locale.identifier])
            if let request = try await AssetInventory.assetInstallationRequest(supporting: [module]) {
                try await request.downloadAndInstall()
            }
        }
        guard await AssetInventory.status(forModules: [module]) == .installed else {
            throw Failure(message: "The speech model for \(locale.identifier) is not installed")
        }
        return analyzer
    }

    func stream() async throws {
        let module = module(live: true)
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [module]) else {
            throw Failure(message: "Apple could not provide a compatible audio format")
        }
        let input = try PCMInput(format: format) {
            try FileHandle.standardInput.read(upToCount: 6_400) ?? Data()
        }
        let sequence = AsyncThrowingStream<AnalyzerInput, Error>(unfolding: {
            try input.next().map { AnalyzerInput(buffer: $0) }
        })
        let analyzer = SpeechAnalyzer(modules: [module])
        try await analyzer.prepareToAnalyze(in: format)
        let results = Task {
            do {
                for try await result in module.results {
                    let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                    let start = CMTimeGetSeconds(result.range.start)
                    let end = CMTimeGetSeconds(CMTimeRangeGetEnd(result.range))
                    guard start.isFinite, end.isFinite else { throw Failure(message: "Invalid Apple transcript timestamps") }
                    emit(["type": result.isFinal ? "final" : "partial", "text": text, "start": start, "end": end])
                }
            } catch {
                emit(["type": "error", "message": error.localizedDescription])
                await analyzer.cancelAndFinishNow()
                throw error
            }
        }
        do {
            emit(["type": "streaming"])
            if let last = try await analyzer.analyzeSequence(sequence) {
                try await analyzer.finalizeAndFinish(through: last)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            try await results.value
        } catch {
            await analyzer.cancelAndFinishNow()
            results.cancel()
            throw error
        }
    }

    func recognize(_ samples: [Float]) async throws -> String {
        let module = module()
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [module]) else {
            throw Failure(message: "Apple could not provide a compatible audio format")
        }
        let audio = try convert(samples, to: format)
        let analyzer = SpeechAnalyzer(modules: [module])
        try await analyzer.prepareToAnalyze(in: format)
        let results = Task { () throws -> String in
            var text = ""
            for try await result in module.results where result.isFinal {
                text += String(result.text.characters)
            }
            return text
        }
        let (input, feed) = AsyncStream<AnalyzerInput>.makeStream()
        feed.yield(AnalyzerInput(buffer: audio))
        feed.finish()
        do {
            if let last = try await analyzer.analyzeSequence(input) {
                try await analyzer.finalizeAndFinish(through: last)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            return try await results.value.trimmingCharacters(in: .whitespacesAndNewlines)
        } catch {
            await analyzer.cancelAndFinishNow()
            results.cancel()
            throw error
        }
    }
}

/// Only the first call to `finish` counts: the recognizer can report again after its final result.
final class Once<T> {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<T, Error>?

    init(_ continuation: CheckedContinuation<T, Error>) {
        self.continuation = continuation
    }

    func finish(_ result: Result<T, Error>) {
        lock.lock()
        let waiting = continuation
        continuation = nil
        lock.unlock()
        waiting?.resume(with: result)
    }
}

final class Legacy: Engine {
    let name = "legacy"
    let recognizer: SFSpeechRecognizer
    var locale: Locale { recognizer.locale }

    init(recognizer: SFSpeechRecognizer) {
        self.recognizer = recognizer
    }

    static func start(_ wanted: Locale) async throws -> Legacy {
        let status = await withCheckedContinuation { done in
            SFSpeechRecognizer.requestAuthorization { done.resume(returning: $0) }
        }
        guard status == .authorized else {
            throw Failure(message: "Speech recognition is not allowed. Turn it on for boardcast in System Settings › Privacy & Security › Speech Recognition.")
        }
        guard let recognizer = SFSpeechRecognizer(locale: wanted), recognizer.isAvailable else {
            throw Failure(message: "Apple speech recognition is not available for \(wanted.identifier)")
        }
        guard recognizer.supportsOnDeviceRecognition else {
            throw Failure(message: "On-device speech recognition is not available for \(wanted.identifier). Choose the bundled speech models instead.")
        }
        // Its answers come on this queue rather than the main one, which nothing here services.
        recognizer.queue = OperationQueue()
        return Legacy(recognizer: recognizer)
    }

    func recognize(_ samples: [Float]) async throws -> String {
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = false
        request.requiresOnDeviceRecognition = true
        if #available(macOS 13.0, *) {
            request.addsPunctuation = true
        }
        request.append(buffer(samples))
        request.endAudio()
        var task: SFSpeechRecognitionTask?
        defer { task?.cancel() }
        return try await withCheckedThrowingContinuation { continuation in
            let once = Once(continuation)
            task = recognizer.recognitionTask(with: request) { result, error in
                if let result, result.isFinal {
                    once.finish(.success(result.bestTranscription.formattedString))
                } else if let error = error as NSError? {
                    // "No speech detected" is an empty sentence, not a failure.
                    once.finish(error.code == 1110 || error.code == 203 ? .success("") : .failure(error))
                }
            }
        }
    }
}

func engine(for wanted: Locale) async throws -> Engine {
    if #available(macOS 26.0, *) {
        do {
            return try await Analyzer.start(wanted)
        } catch {
            emit(["type": "fallback", "message": error.localizedDescription])
        }
    }
    return try await Legacy.start(wanted)
}

func serve(_ identifier: String) async throws {
    let wanted = identifier == "auto" ? Locale.current : Locale(identifier: identifier)
    let engine = try await engine(for: wanted)
    emit(["type": "ready", "engine": engine.name, "locale": engine.locale.identifier, "streaming": engine.name == "analyzer"])
    while true {
        switch try nextInput() {
        case .end: return
        case .stream:
            guard #available(macOS 26.0, *), let analyzer = engine as? Analyzer else {
                throw Failure(message: "This speech engine cannot stream")
            }
            try await analyzer.stream()
            emit(["type": "done"])
            return
        case .sentence(let samples):
            do {
                emit(["type": "text", "text": try await engine.recognize(samples)])
            } catch {
                emit(["type": "error", "message": error.localizedDescription])
            }
        }
    }
}

func selfTest() throws {
    let values: [Float] = [0.25, -0.5, 0.75, 0]
    let same = try convert(values, to: inputFormat)
    guard Array(UnsafeBufferPointer(start: same.floatChannelData![0], count: Int(same.frameLength))) == values else {
        throw Failure(message: "Audio buffer self-test failed")
    }
    let doubled = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 32_000, channels: 1, interleaved: false)!
    let resampled = try convert([Float](repeating: 0.1, count: 1600), to: doubled)
    guard resampled.frameLength > 2_800 && resampled.frameLength <= 3_264 else {
        throw Failure(message: "Resampling self-test failed: \(resampled.frameLength) frames")
    }
    let data = values.withUnsafeBytes { Data($0) }
    var chunks = [Data(data.prefix(3)), Data(data.dropFirst(3).prefix(7)), Data(data.dropFirst(10)), Data()]
    let input = try PCMInput(format: inputFormat) { chunks.removeFirst() }
    var actual: [Float] = []
    while let buffer = try input.next() {
        actual += Array(UnsafeBufferPointer(start: buffer.floatChannelData![0], count: Int(buffer.frameLength)))
    }
    guard actual == values else { throw Failure(message: "PCM split-read self-test failed") }
    var truncated = [Data([0, 0, 0]), Data()]
    let broken = try PCMInput(format: inputFormat) { truncated.removeFirst() }
    do {
        _ = try broken.next()
        throw Failure(message: "Truncated PCM was accepted")
    } catch let error as Failure where error.message == "Incomplete float32 audio sample" {}
    var invalid = [Float.nan].withUnsafeBytes { Data($0) }
    let nonfinite = try PCMInput(format: inputFormat) { defer { invalid = Data() }; return invalid }
    do {
        _ = try nonfinite.next()
        throw Failure(message: "Nonfinite PCM was accepted")
    } catch let error as Failure where error.message == "Invalid streaming samples" {}
    emit(["type": "done"])
}

@main
struct Main {
    static func main() async {
        // Leave if boardcast goes away without closing stdin.
        let parent = getppid()
        let watchdog = DispatchSource.makeTimerSource(queue: .global())
        watchdog.schedule(deadline: .now() + 1, repeating: 1)
        watchdog.setEventHandler { if getppid() != parent { exit(0) } }
        watchdog.resume()
        do {
            let args = Array(CommandLine.arguments.dropFirst())
            switch args.first {
            case "self-test": try systemAudioSelfTest(); try selfTest()
            case "system-audio": try await captureSystemAudio()
            case "serve" where args.count == 2: try await serve(args[1])
            default: throw Failure(message: "Usage: boardcast-speech serve <locale|auto> | system-audio | self-test")
            }
        } catch {
            emit(["type": "error", "message": error.localizedDescription])
            exit(1)
        }
    }
}
