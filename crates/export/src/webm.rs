//! A WebM file around AV1 frames: the smallest part of Matroska a player needs for one video track, written
//! in memory. It follows the AV1 mapping for Matroska (https://github.com/ietf-wg-cellar/matroska-specification,
//! codec/av1.md): the track's CodecPrivate is the av1C record with the sequence header OBU after it, and each
//! block is one temporal unit without its temporal delimiter.

/// A frame as the encoder gave it: when it shows, in ms, whether a player can start there, and its OBUs.
pub struct Frame {
    pub ms: u64,
    pub key: bool,
    pub data: Vec<u8>,
}

/// The video track: its size, its frames a second when they come evenly (a video that only stores a frame
/// when the picture changes has none), the encoder's AV1CodecConfigurationRecord, and its frames.
pub struct Video<'a> {
    pub width: u32,
    pub height: u32,
    pub fps: Option<u32>,
    pub av1c: &'a [u8],
    pub frames: &'a [Frame],
}

/// The sound track, Opus as the Matroska mapping has it (codec/opus.md): its identification header as the
/// codec's private data, the look-ahead a player drops as the track's codec delay, and evenly long packets.
pub struct Audio<'a> {
    pub head: &'a [u8],
    pub pre_skip_ns: u64,
    pub packet_ms: u64,
    pub packets: &'a [Vec<u8>],
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

/// A whole file: a video track, a sound track or both, and how long it runs in ms.
pub fn write(video: Option<Video>, audio: Option<Audio>, duration: f64) -> Vec<u8> {
    let header = element(
        0x1A45_DFA3,
        &[uint(0x4286, 1), uint(0x42F7, 1), uint(0x42F2, 4), uint(0x42F3, 8), text(0x4282, "webm"), uint(0x4287, 4), uint(0x4285, 2)].concat(),
    );
    let info = element(
        0x1549_A966,
        &[uint(0x2A_D7B1, 1_000_000), text(0x4D80, "boardcast"), text(0x5741, "boardcast"), element(0x4489, &duration.to_be_bytes())].concat(),
    );
    let mut tracks = vec![];
    // Every block, as (when, which track, a key frame, its data), to be laid out in time order.
    let mut blocks: Vec<(u64, u64, bool, Vec<u8>)> = vec![];
    let mut number = 0;
    if let Some(v) = &video {
        number += 1;
        // The sequence header goes after the record, as the mapping asks; the first frame carries it.
        let mut private = v.av1c.to_vec();
        if let Some((_, header)) = v.frames.first().and_then(|f| obus(&f.data).into_iter().find(|(kind, _)| *kind == SEQUENCE_HEADER)) {
            private.extend_from_slice(header);
        }
        // BT.709, limited range, as the frames are converted.
        let colour = element(0x55B0, &[uint(0x55B1, 1), uint(0x55B9, 1), uint(0x55BA, 1), uint(0x55BB, 1)].concat());
        let picture = element(0xE0, &[uint(0xB0, u64::from(v.width)), uint(0xBA, u64::from(v.height)), colour].concat());
        let every = v.fps.map(|fps| uint(0x23_E383, 1_000_000_000 / u64::from(fps))).unwrap_or_default();
        tracks.extend(element(0xAE, &[uint(0xD7, number), uint(0x73C5, number), uint(0x83, 1), uint(0x9C, 0), text(0x86, "V_AV1"), element(0x63A2, &private), every, picture].concat()));
        for f in v.frames {
            let data: Vec<u8> = obus(&f.data).into_iter().filter(|(kind, _)| *kind != TEMPORAL_DELIMITER).flat_map(|(_, obu)| obu.to_vec()).collect();
            blocks.push((f.ms, number, f.key, data));
        }
    }
    if let Some(a) = &audio {
        number += 1;
        let sound = element(0xE1, &[element(0xB5, &48_000.0_f64.to_be_bytes()), uint(0x9F, 1)].concat());
        tracks.extend(element(
            0xAE,
            &[uint(0xD7, number), uint(0x73C5, number), uint(0x83, 2), text(0x86, "A_OPUS"), element(0x63A2, a.head), uint(0x56AA, a.pre_skip_ns), uint(0x56BB, 80_000_000), sound].concat(),
        ));
        for (i, packet) in a.packets.iter().enumerate() {
            blocks.push((i as u64 * a.packet_ms, number, true, packet.clone()));
        }
    }
    // In time order, the picture before the sound at the same moment.
    blocks.sort_by_key(|(ms, track, _, _)| (*ms, *track));
    let tracks = element(0x1654_AE6B, &tracks);
    // A cluster starts at each key frame of the video, or after five seconds, so a block's time fits its 16 bits.
    let video_track = u64::from(video.is_some());
    let mut clusters = vec![];
    let mut cluster: Vec<u8> = vec![];
    let mut start = 0_u64;
    for (t, track, key, data) in blocks {
        if cluster.is_empty() || (key && track == video_track) || t - start > 5000 {
            if !cluster.is_empty() {
                clusters.extend(element(0x1F43_B675, &cluster));
            }
            start = t;
            cluster = uint(0xE7, start);
        }
        let mut block = size(track);
        block.extend(((t - start) as i16).to_be_bytes());
        block.push(if key { 0x80 } else { 0x00 });
        block.extend(data);
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
        let frames = [Frame { ms: 0, key: true, data: unit.to_vec() }];
        let file = write(Some(Video { width: 64, height: 48, fps: Some(10), av1c: &[0x81, 0x00, 0x0C, 0x00], frames: &frames }), None, 100.0);
        let find = |needle: &[u8]| file.windows(needle.len()).position(|w| w == needle);
        assert!(find(b"webm").is_some() && find(b"V_AV1").is_some());
        // The record, then the sequence header, in the codec's private data.
        assert!(find(&[0x81, 0x00, 0x0C, 0x00, 0x0A, 0x02, 0xAA, 0xBB]).is_some());
        // The block: track 1, at 0 ms, a key frame, and no temporal delimiter.
        assert!(find(&[0x81, 0x00, 0x00, 0x80, 0x0A, 0x02, 0xAA, 0xBB, 0x32, 0x03, 0x01, 0x02, 0x03]).is_some());
        assert!(find(&[0x80, 0x12, 0x00]).is_none());
    }

    #[test]
    fn sound_goes_beside_the_picture_in_time_order() {
        let frames = [Frame { ms: 0, key: true, data: vec![0x32, 0x01, 0xA1] }, Frame { ms: 30, key: false, data: vec![0x32, 0x01, 0xA2] }];
        let packets = vec![vec![0xB1], vec![0xB2], vec![0xB3]];
        let file = write(
            Some(Video { width: 64, height: 48, fps: None, av1c: &[0x81], frames: &frames }),
            Some(Audio { head: b"OpusHead", pre_skip_ns: 6_500_000, packet_ms: 20, packets: &packets }),
            60.0,
        );
        let find = |needle: &[u8]| file.windows(needle.len()).position(|w| w == needle).unwrap_or_else(|| panic!("{needle:02x?}"));
        find(b"A_OPUS");
        find(b"OpusHead");
        // Picture at 0, sound at 0, 20, picture at 30, sound at 40: track, time, flags, data.
        let order = [
            find(&[0x81, 0x00, 0x00, 0x80, 0x32, 0x01, 0xA1]),
            find(&[0x82, 0x00, 0x00, 0x80, 0xB1]),
            find(&[0x82, 0x00, 0x14, 0x80, 0xB2]),
            find(&[0x81, 0x00, 0x1E, 0x00, 0x32, 0x01, 0xA2]),
            find(&[0x82, 0x00, 0x28, 0x80, 0xB3]),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    }

    #[test]
    fn sound_alone_is_track_one() {
        let packets = vec![vec![0xB1]];
        let file = write(None, Some(Audio { head: b"OpusHead", pre_skip_ns: 0, packet_ms: 20, packets: &packets }), 20.0);
        assert!(file.windows(5).any(|w| w == [0x81, 0x00, 0x00, 0x80, 0xB1]));
        assert!(!file.windows(5).any(|w| w == b"V_AV1"));
    }
}
