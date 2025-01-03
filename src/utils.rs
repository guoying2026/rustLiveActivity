// src/utils.rs
pub fn format_decimal(number: f64) -> String {
    let s = format!("{:.20}", number);
    let s = s.trim_end_matches('0').to_string();
    let s = s.trim_end_matches('.').to_string();

    if let Some(dot_index) = s.find('.') {
        let (int_part, dec_part) = s.split_at(dot_index);
        let dec_part = &dec_part[1..]; // 移除点

        // 计算连续前导零的数量
        let zero_count = dec_part.chars().take_while(|&c| c == '0').count();

        if zero_count >= 3 {
            let remaining = &dec_part[zero_count..];
            let formatted = format!(
                "{}.0{{{}}}{}",
                int_part,
                zero_count - 1,
                remaining
            );
            if formatted.len() > 7 {
                return (&formatted[..7]).parse().unwrap();
            }
            return formatted;
        }

        if s.len() > 7 {
            return (&s[..7]).parse().unwrap();
        }
    }

    s
}

// pub fn deal_number(v: f64) -> String {
//     if v >= 1_000_000_000_000_000.0 { // Quadrillion
//         format!("${:.2}Q", v / 1_000_000_000_000_000.0)
//     } else if v >= 1_000_000_000_000.0 { // Trillion
//         format!("${:.2}T", v / 1_000_000_000_000.0)
//     } else if v >= 1_000_000_000.0 { // Billion
//         format!("${:.2}B", v / 1_000_000_000.0)
//     } else if v >= 1_000_000.0 { // Million
//         format!("${:.2}M", v / 1_000_000.0)
//     } else if v >= 1_000.0 { // Thousand
//         format!("${:.2}K", v / 1_000.0)
//     } else { // Below Thousand
//         format!("${:.2}", v)
//     }
// }
// 
// pub fn format_percentage(value: &str) -> String {
//     // 尝试将字符串转换为 f64
//     match value.parse::<f64>() {
//         Ok(v) => {
//             if v == 0.0 {
//                 "0".to_string()
//             } else {
//                 // 使用 {:.2} 保留两位小数
//                 let mut s = format!("{:.2}", v.abs());
// 
//                 // 去除尾部的零和可能的点
//                 if s.contains('.') {
//                     while s.ends_with('0') {
//                         s.pop();
//                     }
//                     if s.ends_with('.') {
//                         s.pop();
//                     }
//                 }
// 
//                 // 添加负号如果原始值为负
//                 if v < 0.0 {
//                     format!("-{}%", s)
//                 } else {
//                     format!("{}%", s)
//                 }
//             }
//         },
//         Err(_) => "0".to_string(),
//     }
// }
