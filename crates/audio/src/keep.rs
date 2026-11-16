//! A meeting's audio kept as it is heard: 16 kHz mono speech as Opus in an Ogg file, about 10 MB an hour.

use anyhow::{Context, Result};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::SAMPLE_RATE;

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
}

impl Keeper {
    /// Start a new file at `path`.
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut encoder = opus::Encoder::new(SAMPLE_RATE as u32, opus::Channels::Mono, opus::Application::Voip).context("starting the Opus encoder")?;
        encoder.set_bitrate(opus::Bitrate::Bits(BITRATE)).context("setting the bitrate")?;
        let pre_skip = encoder.get_lookahead().unwrap_or(0).max(0) as u16;
        let mut writer = PacketWriter::new(BufWriter::new(file));
        // The identification header: version 1, one channel, the encoder's look-ahead (at 48 kHz), the input rate.
        let mut head = b"OpusHead".to_vec();
        head.push(1);
        head.push(1);
        head.extend_from_slice(&(pre_skip * GRANULES_PER_SAMPLE as u16).to_le_bytes());
        head.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
        head.extend_from_slice(&0_i16.to_le_bytes());
        head.push(0);
        writer.write_packet(head, SERIAL, PacketWriteEndInfo::EndPage, 0).context("writing the Opus header")?;
        let vendor = b"boardcast";
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0_u32.to_le_bytes());
        writer.write_packet(tags, SERIAL, PacketWriteEndInfo::EndPage, 0).context("writing the Opus tags")?;
        Ok(Self { encoder, writer, waiting: vec![], written: 0 })
    }

    /// Keep these samples, as many whole frames as they make; the rest waits for more.
    pub fn write(&mut self, samples: &[f32]) -> Result<()> {
        self.waiting.extend_from_slice(samples);
        while self.waiting.len() >= FRAME {
            let frame: Vec<f32> = self.waiting.drain(..FRAME).collect();
            self.frame(&frame, false)?;
        }
        Ok(())
    }

    fn frame(&mut self, frame: &[f32], last: bool) -> Result<()> {
        let packet = self.encoder.encode_vec_float(frame, 4000).context("encoding the audio")?;
        self.written += frame.len() as u64;
        let end = if last { PacketWriteEndInfo::EndStream } else { PacketWriteEndInfo::NormalPacket };
        self.writer.write_packet(packet, SERIAL, end, self.written * GRANULES_PER_SAMPLE).context("writing the audio")
    }

    /// Finish the file, and say how many seconds it holds.
    pub fn finish(mut self) -> Result<f64> {
        let mut last = std::mem::take(&mut self.waiting);
        let heard = self.written + last.len() as u64;
        last.resize(FRAME, 0.0);
        self.frame(&last, true)?;
        let mut file = self.writer.into_inner();
        std::io::Write::flush(&mut file).context("saving the audio")?;
        Ok(heard as f64 / f64::from(SAMPLE_RATE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // BOARDCAST_KEEP_TEST leaves the file, to be checked with a real decoder.
        if std::env::var_os("BOARDCAST_KEEP_TEST").is_none() {
            let _ = std::fs::remove_file(&path);
        }
    }
}
