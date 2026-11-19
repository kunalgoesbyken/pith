pub fn is_runtime(_exe: &str) -> bool {
    false
}

pub fn entry_of<'a>(_exe: &str, _arguments: &'a [String]) -> Option<&'a str> {
    None
}

pub fn alias_of(_id: &str) -> Option<&'static str> {
    None
}

pub fn is_generic_entry(_stem: &str) -> bool {
    false
}

pub fn is_unlisted_id(_app: &str) -> bool {
    false
}

pub fn inserts_by_paste(_app: &str) -> bool {
    false
}
