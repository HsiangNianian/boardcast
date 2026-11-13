//! A WebM file around AV1 frames: the smallest part of Matroska a player needs for one video track, written
//! in memory. It follows the AV1 mapping for Matroska (https://github.com/ietf-wg-cellar/matroska-specification,
//! codec/av1.md): the track's CodecPrivate is the av1C record with the sequence header OBU after it, and each
//! block is one temporal unit without its temporal delimiter.

/// A frame as the encoder gave it: which input frame it shows, whether a player can start there, and its OBUs.
pub struct Frame {
    pub number: u64,
    pub key: bool,
    pub data: Vec<u8>,
}

/// One element: its ID, as the specification writes it with its length marker, its size, its content.
fn element(id: u32, body: &[u8]) -> Vec<u8> {
    let mut out = id.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect::<Vec<u8>>();
    out.extend(size(body.len() as u64));
    out.extend_from_slice(body);
    out
}

/// A size as an EBML variable-length integer, in as few bytes as it fits.
fn size(n: u64) -> Vec<u8> {
    let length = (1..=8).find(|l| n < (1 << (7 * l)) - 1).expect("a size of under 2^56");
    let marked = n | (1 << (7 * length));
    marked.to_be_bytes()[8 - length..].to_vec()
}

fn uint(id: u32, n: u64) -> Vec<u8> {
    let bytes: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
    element(id, if bytes.is_empty() { &[0] } else { &bytes })
}

fn text(id: u32, s: &str) -> Vec<u8> {
    element(id, s.as_bytes())
}

/// The OBUs of a temporal unit as (type, the whole OBU), read from their headers and sizes.
fn obus(data: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = vec![];
    let mut at = 0;
    while at < data.len() {
        let header = data[at];
        let kind = (header >> 3) & 0x0f;
        let extension = header & 0x04 != 0;
        let has_size = header & 0x02 != 0;
        let mut i = at + 1 + usize::from(extension);
        let payload = if has_size {
            // leb128
            let (mut value, mut shift) = (0_usize, 0);
            while i < data.len() {
                let b = data[i];
                i += 1;
                value |= usize::from(b & 0x7f) << shift;
                shift += 7;
                if b & 0x80 == 0 {
                    break;
                }
            }
            value
        } else {
            data.len() - i
        };
        let end = (i + payload).min(data.len());
        out.push((kind, &data[at..end]));
        at = end;
    }
    out
}

const TEMPORAL_DELIMITER: u8 = 2;
const SEQUENCE_HEADER: u8 = 1;

/// A whole file: `av1c` is the encoder's AV1CodecConfigurationRecord, frames come at `fps` a second.
pub fn write(width: u32, height: u32, fps: u32, av1c: &[u8], frames: &[Frame]) -> Vec<u8> {
    let ms = |frame: u64| frame * 1000 / u64::from(fps);
    // The sequence header goes after the record, as the mapping asks; the first frame carries it.
    let mut private = av1c.to_vec();
    if let Some((_, header)) = frames.first().and_then(|f| obus(&f.data).into_iter().find(|(kind, _)| *kind == SEQUENCE_HEADER)) {
        private.extend_from_slice(header);
    }
    let header = element(
        0x1A45_DFA3,
        &[uint(0x4286, 1), uint(0x42F7, 1), uint(0x42F2, 4), uint(0x42F3, 8), text(0x4282, "webm"), uint(0x4287, 4), uint(0x4285, 2)].concat(),
    );
    let duration = ms(frames.iter().map(|f| f.number + 1).max().unwrap_or(0)) as f64;
    let info = element(
        0x1549_A966,
        &[uint(0x2A_D7B1, 1_000_000), text(0x4D80, "boardcast"), text(0x5741, "boardcast"), element(0x4489, &duration.to_be_bytes())].concat(),
    );
    // BT.709, limited range, as the frames are converted.
    let colour = element(0x55B0, &[uint(0x55B1, 1), uint(0x55B9, 1), uint(0x55BA, 1), uint(0x55BB, 1)].concat());
    let video = element(0xE0, &[uint(0xB0, u64::from(width)), uint(0xBA, u64::from(height)), colour].concat());
    let track = element(
        0xAE,
        &[uint(0xD7, 1), uint(0x73C5, 1), uint(0x83, 1), uint(0x9C, 0), text(0x86, "V_AV1"), element(0x63A2, &private), uint(0x23_E383, 1_000_000_000 / u64::from(fps)), video].concat(),
    );
    let tracks = element(0x1654_AE6B, &track);
    // A cluster starts at each key frame, or after five seconds, so a block's time fits its 16 bits.
    let mut clusters = vec![];
    let mut cluster: Vec<u8> = vec![];
    let mut start = 0_u64;
    for f in frames {
        let t = ms(f.number);
        if cluster.is_empty() || f.key || t - start > 5000 {
            if !cluster.is_empty() {
                clusters.extend(element(0x1F43_B675, &cluster));
            }
            start = t;
            cluster = uint(0xE7, start);
        }
        let mut block = vec![0x81];
        block.extend(((t - start) as i16).to_be_bytes());
        block.push(if f.key { 0x80 } else { 0x00 });
        for (kind, obu) in obus(&f.data) {
            if kind != TEMPORAL_DELIMITER {
                block.extend_from_slice(obu);
            }
        }
        cluster.extend(element(0xA3, &block));
    }
    if !cluster.is_empty() {
        clusters.extend(element(0x1F43_B675, &cluster));
    }
    let segment = element(0x1853_8067, &[info, tracks, clusters].concat());
    [header, segment].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_take_as_few_bytes_as_they_fit() {
        assert_eq!(size(0), vec![0x80]);
        assert_eq!(size(126), vec![0xFE]);
        assert_eq!(size(127), vec![0x40, 0x7F], "all ones in one byte means unknown, so 127 takes two");
        assert_eq!(size(300), vec![0x41, 0x2C]);
    }

    #[test]
    fn temporal_delimiters_go_and_the_sequence_header_joins_the_record() {
        // A temporal delimiter, a sequence header with two bytes, a frame OBU with three.
        let unit = [0x12, 0x00, 0x0A, 0x02, 0xAA, 0xBB, 0x32, 0x03, 0x01, 0x02, 0x03];
        let file = write(64, 48, 10, &[0x81, 0x00, 0x0C, 0x00], &[Frame { number: 0, key: true, data: unit.to_vec() }]);
        let find = |needle: &[u8]| file.windows(needle.len()).position(|w| w == needle);
        assert!(find(b"webm").is_some() && find(b"V_AV1").is_some());
        // The record, then the sequence header, in the codec's private data.
        assert!(find(&[0x81, 0x00, 0x0C, 0x00, 0x0A, 0x02, 0xAA, 0xBB]).is_some());
        // The block: track 1, at 0 ms, a key frame, and no temporal delimiter.
        assert!(find(&[0x81, 0x00, 0x00, 0x80, 0x0A, 0x02, 0xAA, 0xBB, 0x32, 0x03, 0x01, 0x02, 0x03]).is_some());
        assert!(find(&[0x80, 0x12, 0x00]).is_none());
    }
}
