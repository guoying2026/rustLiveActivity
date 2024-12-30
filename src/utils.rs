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
            return formatted;
        }
    }

    s
}