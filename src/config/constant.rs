use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    pub static ref MODEL_TO_PRICE: HashMap<&'static str, i64> = {
        let mut m = HashMap::new();
        m.insert("gpt-4o", 15);
        m.insert("gpt-4o-2024-05-13", 15);
        m.insert("gpt-4o-2024-08-06", 15);
        m.insert("gpt-4o-mini", 1);
        m
    };
}
