fn main() {
    // 打包进去的 llama-server 由 scripts/fetch-llama.mjs 下载到 llama/。
    // 单独跑 cargo build / cargo test 时它可能还不存在，tauri-build 会因为资源目录缺失直接报错，
    // 所以先建一个空目录兜底；真正打包时 beforeBuildCommand 会先把它填好。
    let _ = std::fs::create_dir_all("llama");
    tauri_build::build()
}
