fn main() {
    // 打包进去的 llama-server 和语音模型由 scripts/fetch-llama.mjs、fetch-voice.mjs 下载到 llama/、voice/。
    // 单独跑 cargo build / cargo test 时它可能还不存在，tauri-build 会因为资源目录缺失直接报错，
    // 所以先建一个空目录兜底；真正打包时 beforeBuildCommand 会先把它填好。
    let _ = std::fs::create_dir_all("llama");
    let _ = std::fs::create_dir_all("voice");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=apple-speech/main.swift");
        println!("cargo:rerun-if-changed=apple-speech/Info.plist");
        println!("cargo:rerun-if-changed=../scripts/build-apple-speech.sh");
        println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
        let target = std::env::var("TARGET").unwrap();
        let status = std::process::Command::new("sh")
            .arg("../scripts/build-apple-speech.sh")
            .arg(target)
            .status()
            .expect("Apple speech helper requires Xcode 26+ (xcrun swiftc)");
        assert!(
            status.success(),
            "Failed to build Apple speech helper; select Xcode 26+ with xcode-select"
        );
    }
    tauri_build::build()
}
