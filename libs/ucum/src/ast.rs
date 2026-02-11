#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitExpr {
    pub numerator: Vec<(Term, i32)>,
    pub denominator: Vec<(Term, i32)>,
}

impl UnitExpr {
    pub fn one() -> Self {
        Self {
            numerator: vec![],
            denominator: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    Atom(Atom),
    Group(Box<UnitExpr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Atom {
    /// A UCUM symbol (e.g. `mg`, `m[Hg]`, `[IU]`, `10*`).
    Symbol(String),
    /// A positive integer scalar (e.g. `12` in `[ligne]/12`).
    Integer(u64),
}
