use pith_core::FieldSnapshot;

#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    Field(Box<FieldSnapshot>),
    Nothing,
}
