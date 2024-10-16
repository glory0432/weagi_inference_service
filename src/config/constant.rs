use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    pub static ref MODEL_TO_PRICE: HashMap<&'static str, i64> = {
        let mut m = HashMap::new();
        m.insert("gpt-4o", 15);
        m.insert("gpt-4o-mini", 1);
        m
    };
}
