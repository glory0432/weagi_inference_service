use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    pub static ref MODEL_TO_PRICE: HashMap<&'static str, f64> = {
        let mut m = HashMap::new();
        m.insert("gpt-4o", 15.0);
        m.insert("gpt-4o-mini", 0.5625);
        m.insert("o1-preview", 29.25);
        m.insert("o1-mini", 5.85);
        m
    };
}
