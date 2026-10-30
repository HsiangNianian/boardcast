//! 识别核心的测试：直接喂 wav，不碰声卡。
//! 需要语音模型和测试音频，放在 `BOARDCAST_VOICE_DIR` 指向的目录里（结构和应用的模型目录一样，
//! 另外放一个 `zh.wav`）；没设这个变量就跳过。CI 里由工作流先下载好。

use super::*;
use sherpa_onnx::Wave;
use std::path::PathBuf;

fn dir() -> Option<PathBuf> {
    std::env::var_os("BOARDCAST_VOICE_DIR").map(PathBuf::from)
}

/// 按 100 ms 一块喂，模拟麦克风
fn run(engine: &mut Engine, samples: &[f32]) -> (Vec<String>, Vec<Heard>) {
    let mut events = vec![];
    for chunk in samples.chunks(SAMPLE_RATE as usize / 10) {
        engine.feed(chunk, &mut events);
    }
    engine.finish(&mut events);
    let mut partials = vec![];
    let mut finals = vec![];
    for e in events {
        match e {
            Event::Partial(t) => partials.push(t),
            Event::Final(h) => finals.push(h),
        }
    }
    (partials, finals)
}

#[test]
fn recognizes_a_chinese_sentence() {
    let Some(dir) = dir() else {
        eprintln!("跳过：没有设置 BOARDCAST_VOICE_DIR");
        return;
    };
    let wave = Wave::read(&dir.join("zh.wav").to_string_lossy()).expect("读不到 zh.wav");
    assert_eq!(wave.sample_rate(), SAMPLE_RATE);
    let mut engine = Engine::load(&dir).expect("模型加载失败");
    let t = Instant::now();
    let (_, finals) = run(&mut engine, wave.samples());
    let text: String = finals.iter().map(|h| h.text.as_str()).collect();
    eprintln!(
        "识别结果：{text}（{:.0} ms，音频 {:.1} 秒）",
        t.elapsed().as_millis(),
        wave.samples().len() as f32 / 16000.0
    );
    assert!(!finals.is_empty(), "没有识别出任何句子");
    // sherpa-onnx 自带的测试音频：「开饭时间早上九点至下午五点」
    assert!(text.contains("开饭时间"), "{text}");
    assert!(
        text.contains('9') && text.contains('5'),
        "数字没有规整成阿拉伯数字：{text}"
    );
    // 时间戳在音频范围内
    let dur = wave.samples().len() as f32 / SAMPLE_RATE as f32;
    assert!(finals
        .iter()
        .all(|h| h.start >= 0.0 && h.end <= dur + 0.1 && h.start < h.end));
}

#[test]
fn two_sentences_with_a_pause_come_out_separately() {
    let Some(dir) = dir() else {
        eprintln!("跳过：没有设置 BOARDCAST_VOICE_DIR");
        return;
    };
    let wave = Wave::read(&dir.join("zh.wav").to_string_lossy()).expect("读不到 zh.wav");
    let s = wave.samples();
    // 同一句话说两遍，中间停 1 秒
    let mut audio = s.to_vec();
    audio.extend(std::iter::repeat_n(0.0, SAMPLE_RATE as usize));
    audio.extend_from_slice(s);
    let mut engine = Engine::load(&dir).expect("模型加载失败");
    let (partials, finals) = run(&mut engine, &audio);
    assert_eq!(
        finals.len(),
        2,
        "{:?}",
        finals.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
    assert!(finals[1].start > finals[0].end);
    // 喂得比实时快，临时字幕不一定出得来，这里只要求不出错
    let _ = partials;
}

#[test]
fn voiceprints_tell_speakers_apart() {
    let Some(dir) = dir() else {
        eprintln!("跳过：没有设置 BOARDCAST_VOICE_DIR");
        return;
    };
    // 3D-Speaker 的测试音频：同一个人的两句话，和另一个人的一句话
    let wav = |n: &str| dir.join(format!("{n}.wav"));
    if !wav("speaker1_a_cn_16k").exists() || !dir.join(voice_files::SPEAKER).exists() {
        eprintln!("跳过：没有声纹模型或测试音频");
        return;
    }
    let engine = Engine::load(&dir).expect("模型加载失败");
    let vp = |n: &str| {
        let w = Wave::read(&wav(n).to_string_lossy()).expect("读不到测试音频");
        engine.voiceprint(w.samples()).expect("没算出声纹")
    };
    let a1 = vp("speaker1_a_cn_16k");
    let a2 = vp("speaker1_b_cn_16k");
    let b = vp("speaker2_a_cn_16k");
    let dot = |x: &[f32], y: &[f32]| x.iter().zip(y).map(|(p, q)| p * q).sum::<f32>();
    let len = dot(&a1, &a1).sqrt();
    assert!((len - 1.0).abs() < 1e-3, "声纹没有归一化：{len}");
    let same = dot(&a1, &a2);
    let other = dot(&a1, &b).max(dot(&a2, &b));
    eprintln!("同一个人 {same:.3}，不同的人 {other:.3}");
    assert!(same > 0.5, "同一个人的相似度太低：{same}");
    assert!(other < 0.4, "不同的人相似度太高：{other}");
    // 太短的句子（「嗯」「对」）不算声纹
    assert!(engine.voiceprint(&[0.0; 4000]).is_none());
}

/// 真的打开默认麦克风收 1 秒，看有没有数据进来。需要声卡，默认跳过：
///   cargo test --manifest-path src-tauri/Cargo.toml --lib mic -- --ignored --nocapture
#[test]
#[ignore]
fn mic_delivers_samples() {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let dev = host.default_input_device().expect("没有默认麦克风");
    let cfg = dev.default_input_config().expect("读不到麦克风配置");
    eprintln!(
        "设备：{}，{:?} {} Hz {} 声道",
        dev.description()
            .map(|d| d.name().to_string())
            .unwrap_or_default(),
        cfg.sample_format(),
        cfg.sample_rate(),
        cfg.channels()
    );
    let got = Arc::new(Mutex::new(0usize));
    let g = got.clone();
    let stream = dev
        .build_input_stream::<f32, _, _>(
            cfg.config(),
            move |d: &[f32], _| *g.lock().unwrap() += d.len(),
            |e| panic!("{e}"),
            None,
        )
        .expect("打不开输入流");
    stream.play().unwrap();
    thread::sleep(Duration::from_secs(1));
    drop(stream);
    let n = *got.lock().unwrap();
    eprintln!("1 秒收到 {n} 个样本");
    assert!(n > 0);
}

#[test]
fn silence_produces_nothing() {
    let Some(dir) = dir() else {
        eprintln!("跳过：没有设置 BOARDCAST_VOICE_DIR");
        return;
    };
    let mut engine = Engine::load(&dir).expect("模型加载失败");
    let (partials, finals) = run(&mut engine, &vec![0.0; SAMPLE_RATE as usize * 3]);
    assert!(partials.is_empty() && finals.is_empty());
}

/// 列出这台机器上能选的麦克风，看有没有混进 ALSA 插件或扬声器回采。要声卡，默认跳过：
///   cargo test --lib list_microphones -- --ignored --nocapture
#[test]
#[ignore]
fn list_microphones() {
    let names = voice_devices();
    println!("{names:#?}");
    assert!(names
        .iter()
        .all(|n| !n.starts_with("Monitor of") && !n.contains("Plugin")));
    // 默认麦克风真的能收到声音帧
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let dev = audio_host().default_input_device().expect("没有麦克风");
    let cfg = dev.default_input_config().expect("读不到麦克风参数");
    let got = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let n = got.clone();
    let stream = dev
        .build_input_stream_raw(
            cfg.config(),
            cfg.sample_format(),
            move |d: &cpal::Data, _: &_| {
                n.fetch_add(d.len(), Ordering::SeqCst);
            },
            |e| panic!("{e}"),
            None,
        )
        .expect("打不开麦克风");
    stream.play().unwrap();
    thread::sleep(Duration::from_secs(1));
    drop(stream);
    println!("{:?} 1 秒收到 {} 个采样", cfg, got.load(Ordering::SeqCst));
    assert!(got.load(Ordering::SeqCst) > 1000);
}
