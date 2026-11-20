//! 发布前验证安装包确实由客户端内置公钥对应的私钥签名。
use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("缺少公钥")?;
    let key = String::from_utf8(STANDARD.decode(key.trim())?)?;
    let key = PublicKey::decode(&key)?;
    let files: Vec<_> = std::env::args().skip(1).collect();
    if files.is_empty() {
        return Err("请传入安装包路径".into());
    }
    for file in files {
        let signature = std::fs::read_to_string(format!("{file}.sig"))?;
        let signature = String::from_utf8(STANDARD.decode(signature.trim())?)?;
        let signature = Signature::decode(&signature)?;
        let content = std::fs::read(&file)?;
        key.verify(&content, &signature, false)?;
        println!("签名验证通过：{file}");
    }
    Ok(())
}
