//! A meeting's audio kept as it is heard: 16 kHz mono speech as Opus in an Ogg file, about 10 MB an hour.

use anyhow::{Context, Result};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Speech is kept at this rate, mono.
pub const SAMPLE_RATE: i32 = 16_000;

/// 20 ms frames, as Opus likes for speech.
const FRAME: usize = SAMPLE_RATE as usize / 50;
/// Ogg Opus counts granules at 48 kHz whatever the rate.
const GRANULES_PER_SAMPLE: u64 = 48_000 / SAMPLE_RATE as u64;
const SERIAL: u32 = 0x626f_6172; // "boar"
/// What Opus is asked for: plenty for speech.
const BITRATE: i32 = 24_000;

pub struct Keeper {
    encoder: opus::Encoder,
    writer: PacketWriter<'static, BufWriter<File>>,
    /// Samples heard but not yet a whole frame.
    waiting: Vec<f32>,
    /// Samples written so far, which is where the file has got to.
    written: u64,
    /// The encoder's look-ahead, which every position in the file counts as well (RFC 7845).
    pre_skip: u64,
}

impl Keeper {
    /// Start a new file at `path`.
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let (encoder, pre_skip) = encoder()?;
        let mut writer = PacketWriter::new(BufWriter::new(file));
        writer.write_packet(opus_head(pre_skip), SERIAL, PacketWriteEndInfo::EndPage, 0).context("writing the Opus header")?;
        let vendor = b"Alcyo";
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0_u32.to_le_bytes());
        writer.write_packet(tags, SERIAL, PacketWriteEndInfo::EndPage, 0).context("writing the Opus tags")?;
        Ok(Self { encoder, writer, waiting: vec![], written: 0, pre_skip: u64::from(pre_skip) })
    }

    /// Keep these samples, as many whole frames as they make; the rest waits for more.
    pub fn write(&mut self, samples: &[f32]) -> Result<()> {
        self.waiting.extend_from_slice(samples);
        while self.waiting.len() >= FRAME {
            let frame: Vec<f32> = self.waiting.drain(..FRAME).collect();
            self.frame(&frame, None)?;
        }
        Ok(())
    }

    /// One frame; the last says how much of it was heard, so the file ends where the sound did.
    fn frame(&mut self, frame: &[f32], heard: Option<u64>) -> Result<()> {
        let packet = self.encoder.encode_vec_float(frame, 4000).context("encoding the audio")?;
        self.written += frame.len() as u64;
        let (end, at) = match heard {
            Some(heard) => (PacketWriteEndInfo::EndStream, heard),
            None => (PacketWriteEndInfo::NormalPacket, self.written),
        };
        self.writer.write_packet(packet, SERIAL, end, (at + self.pre_skip) * GRANULES_PER_SAMPLE).context("writing the audio")
    }

    /// Finish the file, and say how many seconds it holds.
    pub fn finish(mut self) -> Result<f64> {
        let mut last = std::mem::take(&mut self.waiting);
        let heard = self.written + last.len() as u64;
        last.resize(FRAME, 0.0);
        self.frame(&last, Some(heard))?;
        let mut file = self.writer.into_inner();
        std::io::Write::flush(&mut file).context("saving the audio")?;
        Ok(heard as f64 / f64::from(SAMPLE_RATE))
    }
}

/// The encoder for speech, and its look-ahead in samples.
fn encoder() -> Result<(opus::Encoder, u16)> {
    let mut encoder = opus::Encoder::new(SAMPLE_RATE as u32, opus::Channels::Mono, opus::Application::Voip).context("starting the Opus encoder")?;
    encoder.set_bitrate(opus::Bitrate::Bits(BITRATE)).context("setting the bitrate")?;
    let pre_skip = encoder.get_lookahead().unwrap_or(0).max(0) as u16;
    Ok((encoder, pre_skip))
}

/// The identification header (RFC 7845): version 1, one channel, the look-ahead (at 48 kHz), the input rate.
fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(1);
    head.extend_from_slice(&(pre_skip * GRANULES_PER_SAMPLE as u16).to_le_bytes());
    head.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
    head.extend_from_slice(&0_i16.to_le_bytes());
    head.push(0);
    head
}

/// Speech as Opus for a file of another kind, such as a video: the identification header that describes
/// it, the look-ahead a player drops from the front, in samples, and a packet for every 20 ms.
pub struct Encoded {
    pub head: Vec<u8>,
    pub pre_skip: u64,
    pub packets: Vec<Vec<u8>>,
}

/// How long each of `encode`'s packets is.
pub const PACKET_MS: u64 = 20;

/// Encode 16 kHz mono samples, the last packet padded with silence.
pub fn encode(samples: &[f32]) -> Result<Encoded> {
    let (mut encoder, pre_skip) = encoder()?;
    let mut packets = Vec::with_capacity(samples.len() / FRAME + 2);
    // The look-ahead's worth of silence after the end, so the last of the sound comes out.
    let tail = vec![0.0; usize::from(pre_skip)];
    let all: Vec<f32> = samples.iter().chain(&tail).copied().collect();
    for chunk in all.chunks(FRAME) {
        let mut frame = chunk.to_vec();
        frame.resize(FRAME, 0.0);
        packets.push(encoder.encode_vec_float(&frame, 4000).context("encoding the audio")?);
    }
    Ok(Encoded { head: opus_head(pre_skip), pre_skip: u64::from(pre_skip), packets })
}

/// A meeting's kept pieces of sound, each `(its file, when it began in ms since the epoch, how long it was
/// meant to be in ms)`, read and laid end to end; with each piece's start and its length as read, in ms.
/// A file that is gone or unreadable is heard as silence of its length, so the rest stays in step.
pub fn read_pieces(files: &[(Option<std::path::PathBuf>, f64, f64)]) -> (Vec<f32>, Vec<(f64, f64)>) {
    let per_ms = f64::from(SAMPLE_RATE) / 1000.0;
    let (mut samples, mut pieces) = (Vec::new(), Vec::new());
    for (path, start, ms) in files {
        let piece = match path.as_deref().map(read) {
            Some(Ok(piece)) => piece,
            Some(Err(e)) => {
                eprintln!("a piece of the meeting's sound could not be read: {e:#}");
                vec![0.0; (ms * per_ms) as usize]
            }
            None => vec![0.0; (ms * per_ms) as usize],
        };
        pieces.push((*start, piece.len() as f64 / per_ms));
        samples.extend(piece);
    }
    (samples, pieces)
}

/// The samples of a kept file, at 16 kHz mono, as they were heard: the encoder's look-ahead taken off the
/// front, and the padding of the last frame off the end.
pub fn read(path: &Path) -> Result<Vec<f32>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut reader = ogg::reading::PacketReader::new(std::io::BufReader::new(file));
    let mut decoder = opus::Decoder::new(SAMPLE_RATE as u32, opus::Channels::Mono).context("starting the Opus decoder")?;
    let (mut samples, mut pre_skip, mut end) = (vec![], 0_usize, None);
    let mut buffer = vec![0.0_f32; FRAME * 6];
    let mut index = 0;
    while let Some(packet) = reader.read_packet().context("reading the audio")? {
        index += 1;
        if index == 1 {
            // OpusHead: the look-ahead at 48 kHz, in bytes 10 and 11.
            if packet.data.len() >= 12 && packet.data.starts_with(b"OpusHead") {
                pre_skip = usize::from(u16::from_le_bytes([packet.data[10], packet.data[11]])) / GRANULES_PER_SAMPLE as usize;
            }
            continue;
        }
        if index == 2 {
            continue;
        }
        let n = decoder.decode_float(&packet.data, &mut buffer, false).context("decoding the audio")?;
        samples.extend_from_slice(&buffer[..n]);
        if packet.last_in_stream() {
            end = Some(packet.absgp_page() / GRANULES_PER_SAMPLE);
        }
    }
    // The last page's position says where the sound ends, look-ahead and all; past it is the last frame's padding.
    if let Some(end) = end {
        samples.truncate((end as usize).min(samples.len()));
    }
    Ok(samples.split_off(pre_skip.min(samples.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_speech_decodes_to_its_length_and_more_is_silence() {
        let tone: Vec<f32> = (0..SAMPLE_RATE as usize + 50).map(|i| (i as f32 * 0.1).sin() * 0.3).collect();
        let encoded = encode(&tone).unwrap();
        assert!(encoded.head.starts_with(b"OpusHead"));
        let mut decoder = opus::Decoder::new(SAMPLE_RATE as u32, opus::Channels::Mono).unwrap();
        let mut out = vec![];
        let mut buffer = vec![0.0_f32; FRAME * 6];
        for p in &encoded.packets {
            let n = decoder.decode_float(p, &mut buffer, false).unwrap();
            out.extend_from_slice(&buffer[..n]);
        }
        let heard = &out[encoded.pre_skip as usize..];
        assert!(heard.len() >= tone.len(), "{} < {}", heard.len(), tone.len());
        let middle = SAMPLE_RATE as usize / 2;
        let error = (-6_i64..=6).map(|lag| (middle..middle + 400).map(|i| (heard[(i as i64 + lag) as usize] - tone[i]).abs()).sum::<f32>() / 400.0).fold(f32::MAX, f32::min);
        assert!(error < 0.02, "{error}");
    }

    #[test]
    fn a_missing_piece_is_silence_of_its_length() {
        let (samples, pieces) = read_pieces(&[(None, 5000.0, 250.0)]);
        assert_eq!(samples.len(), 4000);
        assert_eq!(pieces, [(5000.0, 250.0)]);
    }

    #[test]
    fn heard_audio_is_kept_as_an_ogg_opus_file_of_its_length() {
        let path = std::env::temp_dir().join(format!("boardcast-keep-{}.opus", std::process::id()));
        let mut keeper = Keeper::create(&path).unwrap();
        // Two and a bit seconds of a tone, in uneven pieces as a microphone sends them.
        let tone: Vec<f32> = (0..SAMPLE_RATE as usize * 2 + 100).map(|i| (i as f32 * 0.1).sin() * 0.3).collect();
        for piece in tone.chunks(1234) {
            keeper.write(piece).unwrap();
        }
        let seconds = keeper.finish().unwrap();
        assert!((seconds - 2.00625).abs() < 1e-9, "{seconds}");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"OggS");
        assert!(bytes.windows(8).any(|w| w == b"OpusHead") && bytes.windows(8).any(|w| w == b"OpusTags"));
        assert!(bytes.len() < 12_000, "about 24 kbit/s: {} bytes for two seconds", bytes.len());
        // It reads back as long as it was heard, and sounds as it did.
        let back = read(&path).unwrap();
        assert_eq!(back.len(), tone.len());
        // Opus shifts the sound by a few samples (a fraction of a millisecond); within that, it is the same.
        let middle = SAMPLE_RATE as usize;
        let error = |lag: i64| (middle..middle + 400).map(|i| (back[(i as i64 + lag) as usize] - tone[i]).abs()).sum::<f32>() / 400.0;
        let best = (-6_i64..=6).map(error).fold(f32::MAX, f32::min);
        assert!(best < 0.02, "mean error {best} at the best of a few samples' shift");
        // BOARDCAST_KEEP_TEST leaves the file, to be checked with a real decoder.
        if std::env::var_os("BOARDCAST_KEEP_TEST").is_none() {
            let _ = std::fs::remove_file(&path);
        }
    }
}
