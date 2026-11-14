//! Validação de entradas e formatação de saídas.

use hdf_core::Kind;
use serde_json::Value;

pub const MAX_TEXT: usize = 200;
pub const MAX_PATH: usize = 4096;
pub const MAX_LIST: usize = 100;

/// Tamanho legível em unidades decimais (como o Finder): "1,5 GB" -> "1.5 GB".
pub fn human_size(b: u64) -> String {
    const U: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if b < 1000 {
        return format!("{b} B");
    }
    let mut v = b as f64 / 1000.0;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

/// Aceita número (bytes) ou texto com unidade: "2GB", "500 MB", "1.5GiB", "1,5 gb".
pub fn parse_size(v: &Value) -> Result<u64, String> {
    const MAX: f64 = 9.0e18;
    let bad = || format!("Tamanho inválido: {v}. Use bytes (número) ou texto como \"2GB\" ou \"500 MB\".");
    let n: f64 = match v {
        Value::Number(n) => n.as_f64().ok_or_else(bad)?,
        Value::String(s) => {
            let s = s.trim();
            let split = s
                .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
                .unwrap_or(s.len());
            let (num, unit) = s.split_at(split);
            let num: f64 = num.replace(',', ".").parse().map_err(|_| bad())?;
            let unit = unit.trim().to_uppercase().replace(' ', "");
            let mult: f64 = match unit.as_str() {
                "" | "B" | "BYTES" | "BYTE" => 1.0,
                "K" | "KB" => 1e3,
                "M" | "MB" => 1e6,
                "G" | "GB" => 1e9,
                "T" | "TB" => 1e12,
                "KIB" => 1024.0,
                "MIB" => 1024f64.powi(2),
                "GIB" => 1024f64.powi(3),
                "TIB" => 1024f64.powi(4),
                _ => return Err(bad()),
            };
            num * mult
        }
        _ => return Err(bad()),
    };
    if !n.is_finite() || !(0.0..MAX).contains(&n) {
        return Err(bad());
    }
    Ok(n.round() as u64)
}

/// "2025-01-31" (ou ISO mais longo, usa só a data). Devolve "AAAA-MM-DD".
pub fn parse_date(s: &str) -> Result<String, String> {
    let bad = || format!("Data inválida: \"{s}\". Use o formato AAAA-MM-DD, por exemplo 2025-01-31.");
    let t = s.trim();
    let b = t.as_bytes();
    if b.len() < 10 || !t.is_char_boundary(10) {
        return Err(bad());
    }
    let ok = b[..10].iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
    if !ok {
        return Err(bad());
    }
    let month: u32 = t[5..7].parse().map_err(|_| bad())?;
    let day: u32 = t[8..10].parse().map_err(|_| bad())?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad());
    }
    Ok(t[..10].to_string())
}

pub fn kind_pt(k: Kind) -> &'static str {
    match k {
        Kind::Folder => "pasta",
        Kind::Video => "video",
        Kind::Image => "imagem",
        Kind::Audio => "audio",
        Kind::Document => "documento",
        Kind::Archive => "compactado",
        Kind::Other => "outro",
    }
}

pub fn parse_kind(s: &str) -> Result<Kind, String> {
    match hdf_core::db::normalize(s.trim()).as_str() {
        "pasta" | "folder" => Ok(Kind::Folder),
        "video" | "videos" => Ok(Kind::Video),
        "imagem" | "imagens" | "image" | "images" | "foto" | "fotos" => Ok(Kind::Image),
        "audio" | "audios" => Ok(Kind::Audio),
        "documento" | "documentos" | "document" => Ok(Kind::Document),
        "compactado" | "compactados" | "archive" | "zip" => Ok(Kind::Archive),
        "outro" | "outros" | "other" => Ok(Kind::Other),
        _ => Err(format!(
            "Tipo inválido: \"{s}\". Use: pasta, video, imagem, audio, documento, compactado ou outro."
        )),
    }
}

// ---- leitura de argumentos ----

pub fn arg<'a>(args: &'a Value, key: &str) -> Option<&'a Value> {
    args.get(key).filter(|v| !v.is_null())
}

pub fn opt_str(args: &Value, key: &str, max: usize) -> Result<Option<String>, String> {
    match arg(args, key) {
        None => Ok(None),
        Some(Value::String(s)) => {
            if s.chars().count() > max {
                return Err(format!("O campo '{key}' é longo demais (máximo {max} caracteres)."));
            }
            if s.contains('\0') {
                return Err(format!("O campo '{key}' tem caracteres inválidos."));
            }
            Ok(Some(s.clone()))
        }
        Some(_) => Err(format!("O campo '{key}' precisa ser texto.")),
    }
}

pub fn opt_int(args: &Value, key: &str, default: u64, min: u64, max: u64) -> Result<u64, String> {
    match arg(args, key) {
        None => Ok(default),
        Some(v) => {
            let n = v
                .as_u64()
                .ok_or_else(|| format!("O campo '{key}' precisa ser um número inteiro (de {min} a {max})."))?;
            if n < min || n > max {
                return Err(format!("O campo '{key}' precisa estar entre {min} e {max} (recebi {n})."));
            }
            Ok(n)
        }
    }
}

/// Lista a partir de um array (ou de um valor único). Máximo [`MAX_LIST`] itens.
pub fn list_arg<'a>(args: &'a Value, key: &str) -> Result<Vec<&'a Value>, String> {
    match arg(args, key) {
        None => Ok(vec![]),
        Some(Value::Array(a)) => {
            if a.len() > MAX_LIST {
                return Err(format!("O campo '{key}' aceita no máximo {MAX_LIST} itens."));
            }
            Ok(a.iter().collect())
        }
        Some(v) => Ok(vec![v]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sizes() {
        assert_eq!(parse_size(&json!("2GB")).unwrap(), 2_000_000_000);
        assert_eq!(parse_size(&json!("1,5 gb")).unwrap(), 1_500_000_000);
        assert_eq!(parse_size(&json!("1GiB")).unwrap(), 1_073_741_824);
        assert_eq!(parse_size(&json!(1234)).unwrap(), 1234);
        assert!(parse_size(&json!("abc")).is_err());
        assert!(parse_size(&json!("-5MB")).is_err());
        assert!(parse_size(&json!(-1)).is_err());
        assert!(parse_size(&json!("5XB")).is_err());
        assert_eq!(human_size(1_500_000), "1.5 MB");
        assert_eq!(human_size(10), "10 B");
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("2025-01-31").unwrap(), "2025-01-31");
        assert_eq!(parse_date("2025-01-31T10:00:00Z").unwrap(), "2025-01-31");
        assert!(parse_date("31/01/2025").is_err());
        assert!(parse_date("2025-13-01").is_err());
        assert!(parse_date("é").is_err());
    }

    #[test]
    fn kinds() {
        assert_eq!(parse_kind("Vídeo").unwrap(), Kind::Video);
        assert_eq!(parse_kind("image").unwrap(), Kind::Image);
        assert!(parse_kind("xyz").is_err());
    }
}
