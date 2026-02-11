use super::BindValue;

pub(super) fn push_text(bind_params: &mut Vec<BindValue>, value: String) -> usize {
    bind_params.push(BindValue::Text(value));
    bind_params.len()
}

pub(super) fn push_text_array(bind_params: &mut Vec<BindValue>, value: Vec<String>) -> usize {
    bind_params.push(BindValue::TextArray(value));
    bind_params.len()
}
