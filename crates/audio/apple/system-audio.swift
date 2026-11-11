import AVFoundation
import CoreGraphics
import Foundation
import ScreenCaptureKit

func systemAudioSamples(_ buffer: CMSampleBuffer) throws -> [Float] {
    guard buffer.isValid,
          let description = buffer.formatDescription?.audioStreamBasicDescription,
          description.mSampleRate == 16_000, description.mChannelsPerFrame == 1,
          description.mFormatID == kAudioFormatLinearPCM,
          description.mFormatFlags & kAudioFormatFlagIsFloat != 0,
          description.mBitsPerChannel == 32 else {
        throw Failure(message: "Unexpected system audio format; expected 16 kHz mono float32")
    }
    return try buffer.withAudioBufferList { buffers, _ in
        let frames = buffer.numSamples
        guard buffers.count == 1, frames > 0, frames <= 16_000,
              Int(buffers[0].mDataByteSize) == frames * MemoryLayout<Float>.size,
              let data = buffers[0].mData else {
            throw Failure(message: "Invalid system audio buffer")
        }
        let samples = Array(UnsafeBufferPointer(start: data.assumingMemoryBound(to: Float.self), count: frames))
        guard samples.allSatisfy(\.isFinite) else { throw Failure(message: "Invalid system audio samples") }
        return samples
    }
}

func systemAudioTime(_ time: CMTime, offset: Int) throws -> Int64 {
    let stamp = CMTimeConvertScale(CMTimeAdd(time, CMTime(value: Int64(offset), timescale: 16_000)),
                                   timescale: 1_000_000_000, method: .roundHalfAwayFromZero)
    guard time.isNumeric, time.epoch == 0, stamp.isNumeric, stamp.timescale == 1_000_000_000, stamp.value >= 0 else {
        throw Failure(message: "Invalid system audio timestamp")
    }
    return stamp.value
}

@available(macOS 13.0, *)
final class SystemAudioOutput: NSObject, SCStreamOutput, SCStreamDelegate {
    let queue = DispatchQueue(label: "boardcast.system-audio")
    // Accessed only on the sample queue so Ready always precedes audio.
    var started = false

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, started, buffer.isValid, buffer.numSamples > 0 else { return }
        do {
            let samples = try systemAudioSamples(buffer)
            let time = CMSampleBufferGetOutputPresentationTimeStamp(buffer)
            for offset in stride(from: 0, to: samples.count, by: 1600) {
                emit(["type": "audio", "hostTimeNs": try systemAudioTime(time, offset: offset),
                      "samples": Array(samples[offset..<min(offset + 1600, samples.count)])])
            }
        } catch {
            emit(["type": "error", "message": error.localizedDescription])
            exit(1)
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        emit(["type": "error", "message": "System audio capture stopped: \(error.localizedDescription)"])
        exit(1)
    }
}

@available(macOS 13.0, *)
func captureSystemAudio() async throws {
    guard CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() else {
        throw Failure(message: "SYSTEM_AUDIO_PERMISSION")
    }
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
    guard let display = content.displays.first else { throw Failure(message: "No display available for system audio capture") }
    let ownApps = content.applications.filter { $0.processID == getpid() || $0.processID == getppid() }
    let filter = SCContentFilter(display: display, excludingApplications: ownApps, exceptingWindows: [])
    let configuration = SCStreamConfiguration()
    configuration.capturesAudio = true
    configuration.excludesCurrentProcessAudio = true
    configuration.sampleRate = 16_000
    configuration.channelCount = 1
    // Only subscribe to audio; minimize the unused display stream.
    configuration.width = 2
    configuration.height = 2
    configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
    let output = SystemAudioOutput()
    let stream = SCStream(filter: filter, configuration: configuration, delegate: output)
    try stream.addStreamOutput(output, type: .audio, sampleHandlerQueue: output.queue)
    try await stream.startCapture()
    output.queue.sync {
        emit(["type": "ready"])
        output.started = true
    }
    // EOF is the parent's stop signal; the existing watchdog also covers a parent crash.
    try await Task.detached {
        while !(try FileHandle.standardInput.read(upToCount: 1) ?? Data()).isEmpty {}
    }.value
    try await stream.stopCapture()
}

func systemAudioSelfTest() throws {
    func sample(_ rate: Double, _ channels: AVAudioChannelCount, _ values: [Float]) throws -> CMSampleBuffer {
        let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: channels)!
        let pcm = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(values.count))!
        pcm.frameLength = AVAudioFrameCount(values.count)
        for channel in 0..<Int(channels) {
            for (index, value) in values.enumerated() { pcm.floatChannelData![channel][index] = value }
        }
        var description: CMAudioFormatDescription?
        guard CMAudioFormatDescriptionCreate(allocator: kCFAllocatorDefault, asbd: format.streamDescription,
                layoutSize: 0, layout: nil, magicCookieSize: 0, magicCookie: nil,
                extensions: nil, formatDescriptionOut: &description) == noErr else {
            throw Failure(message: "Cannot create test audio format")
        }
        var timing = CMSampleTimingInfo(duration: CMTime(value: 1, timescale: CMTimeScale(rate)),
                                       presentationTimeStamp: .zero, decodeTimeStamp: .invalid)
        var buffer: CMSampleBuffer?
        guard CMSampleBufferCreate(allocator: kCFAllocatorDefault, dataBuffer: nil, dataReady: false,
                makeDataReadyCallback: nil, refcon: nil, formatDescription: description,
                sampleCount: values.count, sampleTimingEntryCount: 1, sampleTimingArray: &timing,
                sampleSizeEntryCount: 0, sampleSizeArray: nil, sampleBufferOut: &buffer) == noErr,
              let buffer,
              CMSampleBufferSetDataBufferFromAudioBufferList(buffer, blockBufferAllocator: kCFAllocatorDefault,
                blockBufferMemoryAllocator: kCFAllocatorDefault, flags: 0, bufferList: pcm.audioBufferList) == noErr,
              CMSampleBufferSetDataReady(buffer) == noErr else {
            throw Failure(message: "Cannot create test audio buffer")
        }
        return buffer
    }
    let values: [Float] = [0, 0.25, -0.5, 1, -1]
    guard try systemAudioSamples(sample(16_000, 1, values)) == values else {
        throw Failure(message: "System audio samples changed")
    }
    let time = CMTime(value: 7, timescale: 3)
    guard try systemAudioTime(time, offset: 1600) - systemAudioTime(time, offset: 0) == 100_000_000,
          try systemAudioTime(CMSampleBufferGetOutputPresentationTimeStamp(sample(16_000, 1, values)), offset: 1) == 62_500 else {
        throw Failure(message: "System audio packet timestamps changed")
    }
    for invalid in [CMTime.invalid, .indefinite, .positiveInfinity, CMTime(value: -1, timescale: 1)] {
        do { _ = try systemAudioTime(invalid, offset: 0) }
        catch { continue }
        throw Failure(message: "Invalid system audio timestamp was accepted")
    }
    for invalid in [try sample(48_000, 1, values), try sample(16_000, 2, values), try sample(16_000, 1, [.nan])] {
        do {
            _ = try systemAudioSamples(invalid)
        } catch { continue }
        throw Failure(message: "Invalid system audio was accepted")
    }
}
