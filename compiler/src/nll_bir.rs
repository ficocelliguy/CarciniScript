//! The borrow checker's per-function IR ("BIR"): locals, places (a local plus projections), a
//! control-flow graph of basic blocks, and the region-typed view of every local (`RTy`). The
//! lowering from syntax is in `nll_lower.rs`; the analysis that runs over it is in `nll.rs`.
//!
//! Everything is indexed densely (`Local`, `Block`, `Region` are `u32`s) so the fixpoints in
//! `nll.rs` run over bitsets, not hash maps.

use std::fmt::Write;

/// An absolute source range.
pub type Span = (usize, usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Local(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Block(pub u32);

/// A lifetime: an inference variable (its value is a set of points) or a universal region (a
/// lifetime of the function's signature).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Region(pub u32);

impl Local {
    pub fn idx(self) -> usize {
        self.0 as usize
    }
}
impl Block {
    pub fn idx(self) -> usize {
        self.0 as usize
    }
}
impl Region {
    pub fn idx(self) -> usize {
        self.0 as usize
    }
}

/// A projection out of a place.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Proj {
    /// `.name` (`.0` for tuples and tuple structs).
    Field(String),
    /// `*`
    Deref,
    /// `[_]` (every index overlaps every other).
    Index,
    /// The enum variant a pattern matched (different variants are disjoint).
    Downcast(String),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Place {
    pub local: Local,
    pub proj: Vec<Proj>,
}

impl Place {
    pub fn local(l: Local) -> Place {
        Place {
            local: l,
            proj: Vec::new(),
        }
    }

    pub fn field(mut self, f: impl Into<String>) -> Place {
        self.proj.push(Proj::Field(f.into()));
        self
    }

    pub fn deref(mut self) -> Place {
        self.proj.push(Proj::Deref);
        self
    }

    pub fn index(mut self) -> Place {
        self.proj.push(Proj::Index);
        self
    }

    pub fn downcast(mut self, v: impl Into<String>) -> Place {
        self.proj.push(Proj::Downcast(v.into()));
        self
    }

    pub fn is_local(&self) -> bool {
        self.proj.is_empty()
    }

    /// Whether the place goes through a `*` (so it names what a reference points at, not the
    /// local's own storage).
    pub fn has_deref(&self) -> bool {
        self.proj.contains(&Proj::Deref)
    }

    /// Whether `self` is a prefix of (or equal to) `other`.
    pub fn is_prefix_of(&self, other: &Place) -> bool {
        self.local == other.local
            && self.proj.len() <= other.proj.len()
            && self.proj.iter().zip(&other.proj).all(|(a, b)| a == b)
    }
}

/// How a statement touches a place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// A read of the whole value (a copy, a `match` scrutinee, a closure reading a capture).
    Read,
    /// A read of the outer layer only (`match` on the discriminant).
    ShallowRead,
    /// Moving the value out.
    Move,
    /// Assigning (the outer layer is overwritten).
    Write,
    /// Taking `&place`.
    Borrow,
    /// Taking `&mut place`, or activating a two-phase one at its call.
    MutBorrow,
}

impl Access {
    /// Whether the access reaches what the place's references point at.
    pub fn is_deep(self) -> bool {
        !matches!(self, Access::ShallowRead | Access::Write)
    }

}

#[derive(Clone, Debug)]
pub struct Operand {
    pub place: Place,
    /// Whether using it moves the value (non-`Copy`).
    pub moves: bool,
    pub span: Span,
}

impl Operand {
    pub fn access(&self) -> Access {
        if self.moves { Access::Move } else { Access::Read }
    }
}

#[derive(Clone, Debug)]
pub enum Rvalue {
    /// A copy or move of a place. (Constants need no rvalue: they're `Rvalue::Ops` of nothing.)
    Use(Operand),
    /// `&place` / `&mut place`. A two-phase borrow (a method call's `&mut` receiver) is shared
    /// until the call that activates it.
    Ref {
        region: Region,
        mutable: bool,
        place: Place,
        two_phase: bool,
    },
    /// A value computed from operands: a call, an aggregate, an operator, a literal.
    Ops {
        args: Vec<Operand>,
        /// Two-phase borrows this call activates, by their region.
        activate: Vec<(Place, Region)>,
    },
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Assign(Place, Rvalue),
    /// The end of a local's scope: borrows of it must have ended.
    StorageDead(Local),
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Term {
    Goto(Block),
    /// A branch on `operand` (read shallowly): every target may follow.
    Switch(Option<Operand>, Vec<Block>),
    Return,
    /// After a panic or an infinite loop's body with no `break`.
    Unreachable,
}

impl Term {
    pub fn successors(&self) -> Vec<Block> {
        match self {
            Term::Goto(b) => vec![*b],
            Term::Switch(_, bs) => bs.clone(),
            Term::Return | Term::Unreachable => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockData {
    pub stmts: Vec<Stmt>,
    pub term: Term,
    pub term_span: Span,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocalKind {
    /// A `let` binding, pattern binding or parameter.
    User,
    /// A compiler temporary.
    Temp,
    /// The function's return slot.
    Return,
}

#[derive(Clone, Debug)]
pub struct LocalData {
    pub name: String,
    pub rty: RTy,
    pub kind: LocalKind,
    /// The declaration (for messages).
    pub span: Span,
    /// Has a destructor (`impl Drop`), so it's used again when its scope ends.
    pub drops: bool,
}

/// `longer: shorter` (`'a: 'b`): every point of `shorter` is in `longer`.
#[derive(Clone, Debug)]
pub struct Constraint {
    pub longer: Region,
    pub shorter: Region,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Universal {
    pub name: String,
    /// The universal regions this one is declared to outlive (`'a: 'b`, `'static` outlives all).
    pub outlives: Vec<Region>,
}

#[derive(Clone, Debug, Default)]
pub struct RegionData {
    pub universal: Option<Universal>,
}

#[derive(Clone, Debug)]
pub struct ReturnSite {
    pub span: Span,
    /// Whether the value returned is a reference itself (not a struct holding one).
    pub direct_ref: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Body {
    pub locals: Vec<LocalData>,
    pub blocks: Vec<BlockData>,
    pub regions: Vec<RegionData>,
    pub constraints: Vec<Constraint>,
    pub returns: Vec<ReturnSite>,
    /// The name of the function, for messages.
    pub name: String,
}

impl Body {
    pub fn new_region(&mut self) -> Region {
        self.regions.push(RegionData::default());
        Region(self.regions.len() as u32 - 1)
    }

    pub fn new_universal(&mut self, name: &str) -> Region {
        self.regions.push(RegionData {
            universal: Some(Universal {
                name: name.to_string(),
                outlives: Vec::new(),
            }),
        });
        Region(self.regions.len() as u32 - 1)
    }

    pub fn outlives(&mut self, longer: Region, shorter: Region, span: Span) {
        if longer != shorter {
            self.constraints.push(Constraint {
                longer,
                shorter,
                span,
            });
        }
    }

    pub fn new_local(&mut self, name: &str, rty: RTy, kind: LocalKind, span: Span) -> Local {
        self.locals.push(LocalData {
            name: name.to_string(),
            rty,
            kind,
            span,
            drops: false,
        });
        Local(self.locals.len() as u32 - 1)
    }

    pub fn new_block(&mut self) -> Block {
        self.blocks.push(BlockData {
            stmts: Vec::new(),
            term: Term::Unreachable,
            term_span: (0, 0),
        });
        Block(self.blocks.len() as u32 - 1)
    }

    /// The number of points: one per statement plus one per terminator.
    pub fn point_count(&self) -> usize {
        self.blocks.iter().map(|b| b.stmts.len() + 1).sum()
    }

    /// The type of a place, by projecting the local's. `field(variant, name, adt)` gives a
    /// struct's or variant's field type.
    pub fn place_rty(&self, p: &Place, field: &dyn Fn(&str, &str, &RTy) -> Option<RTy>) -> RTy {
        let mut t = self.locals[p.local.idx()].rty.clone();
        let mut variant: Option<String> = None;
        for proj in &p.proj {
            t = match proj {
                Proj::Deref => match t {
                    RTy::Ref(_, _, inner) => *inner,
                    RTy::Cell(inner) => *inner,
                    _ => RTy::Opaque(false),
                },
                Proj::Index => match t {
                    RTy::Vec(e) => *e,
                    RTy::Str => RTy::Str,
                    _ => RTy::Opaque(false),
                },
                Proj::Downcast(v) => {
                    variant = Some(v.clone());
                    t
                }
                Proj::Field(f) => {
                    let out = match &t {
                        RTy::Tuple(es) => f.parse::<usize>().ok().and_then(|i| es.get(i).cloned()),
                        RTy::Option(inner) => Some((**inner).clone()),
                        RTy::Result(a, b) => Some(match variant.as_deref() {
                            Some("Err") => (**b).clone(),
                            _ => (**a).clone(),
                        }),
                        RTy::Adt(..) => {
                            let v = variant.take().unwrap_or_default();
                            field(&v, f, &t)
                        }
                        _ => None,
                    };
                    variant = None;
                    out.unwrap_or(RTy::Opaque(false))
                }
            };
        }
        t
    }

    /// A place for messages, as rustc spells it: `v`, `s.a`, `*r`, `v[_]`; a deref before a field
    /// is left out (`self.items`, not `(*self).items`).
    pub fn describe(&self, p: &Place) -> String {
        let mut s = self.locals[p.local.idx()].name.clone();
        let mut i = 0;
        while i < p.proj.len() {
            match &p.proj[i] {
                Proj::Deref => {
                    let next = p.proj.get(i + 1);
                    if !matches!(next, Some(Proj::Field(_)) | Some(Proj::Index)) {
                        s = format!("*{s}");
                    }
                }
                Proj::Field(f) => {
                    let _ = write!(s, ".{f}");
                }
                Proj::Index => s.push_str("[_]"),
                Proj::Downcast(_) => {}
            }
            i += 1;
        }
        s
    }
}

/// Whether an access to `access` conflicts with a borrow of `borrowed`, by rustc's rules
/// (`places_conflict`): disjoint fields and variants never conflict; a shallow access to a
/// prefix doesn't reach through a `*`; nothing reaches through a `*` of a shared reference.
/// `shared_deref(i)` says whether the borrowed place's `i`th projection derefs a shared
/// reference.
pub fn places_conflict(
    borrowed: &Place,
    access: &Place,
    deep: bool,
    shared_deref: &dyn Fn(usize) -> bool,
) -> bool {
    if borrowed.local != access.local {
        return false;
    }
    let n = borrowed.proj.len().min(access.proj.len());
    for i in 0..n {
        match (&borrowed.proj[i], &access.proj[i]) {
            (Proj::Field(a), Proj::Field(b)) if a != b => return false,
            (Proj::Downcast(a), Proj::Downcast(b)) if a != b => return false,
            _ => {}
        }
    }
    if borrowed.proj.len() > n {
        // The borrow is deeper: does the access reach it?
        for (i, p) in borrowed.proj.iter().enumerate().skip(n) {
            if *p == Proj::Deref && (!deep || shared_deref(i)) {
                return false;
            }
        }
    }
    true
}

/// A set of `u32`s as packed bits.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub fn new(n: usize) -> BitSet {
        BitSet {
            words: vec![0; n.div_ceil(64)],
        }
    }

    pub fn contains(&self, i: usize) -> bool {
        self.words
            .get(i / 64)
            .is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    pub fn insert(&mut self, i: usize) -> bool {
        let w = i / 64;
        if w >= self.words.len() {
            self.words.resize(w + 1, 0);
        }
        let bit = 1u64 << (i % 64);
        let was = self.words[w] & bit != 0;
        self.words[w] |= bit;
        !was
    }

    pub fn remove(&mut self, i: usize) {
        if let Some(w) = self.words.get_mut(i / 64) {
            *w &= !(1u64 << (i % 64));
        }
    }

    /// `self |= other`; whether anything changed.
    pub fn union_with(&mut self, other: &BitSet) -> bool {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        let mut changed = false;
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            let n = *a | *b;
            changed |= n != *a;
            *a = n;
        }
        changed
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, w)| {
            let mut w = *w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let b = w.trailing_zeros() as usize;
                w &= w - 1;
                Some(wi * 64 + b)
            })
        })
    }
}

/// A type with its references and lifetimes kept (the type checker's `Ty` erases them). Only
/// the shapes the borrow checker follows are spelled out; everything else is `Opaque`, which
/// holds no references and so is never borrowed through.
#[derive(Clone, PartialEq, Debug)]
pub enum RTy {
    /// `&'r T` / `&'r mut T`.
    Ref(Region, bool, Box<RTy>),
    /// A struct or enum of this file, with its type and lifetime arguments.
    Adt(String, Vec<RTy>, Vec<Region>),
    Tuple(Vec<RTy>),
    Option(Box<RTy>),
    Result(Box<RTy>, Box<RTy>),
    /// `Vec<T>`, `[T; N]`, `[T]`, `VecDeque<T>`, ..
    Vec(Box<RTy>),
    Set(Box<RTy>),
    Map(Box<RTy>, Box<RTy>),
    /// `RefCell<T>` / `Cell<T>` / `Mutex<T>`: invariant in `T`.
    Cell(Box<RTy>),
    /// A std iterator, entry, guard, or adapter: what it borrows (if anything), whether mutably,
    /// and what it yields (or, for an entry, what it hands out).
    Iter {
        region: Option<Region>,
        mutable: bool,
        elem: Box<RTy>,
    },
    /// A closure: what it captures by reference, and what calling it returns.
    Closure {
        region: Option<Region>,
        ret: Box<RTy>,
    },
    /// `String`: owned, moved by a by-value use.
    String,
    /// `str`, or a `String` that may be a `&str` (the type checker makes them one type): never
    /// moved.
    Str,
    /// A type with no references inside. The flag: certainly owned and not `Copy` (a by-value
    /// use moves it).
    Opaque(bool),
}

impl RTy {
    pub fn opaque() -> RTy {
        RTy::Opaque(false)
    }

    /// Every region the type mentions.
    pub fn regions(&self, out: &mut Vec<Region>) {
        match self {
            RTy::Ref(r, _, inner) => {
                out.push(*r);
                inner.regions(out);
            }
            RTy::Adt(_, args, rs) => {
                out.extend(rs.iter().copied());
                for a in args {
                    a.regions(out);
                }
            }
            RTy::Tuple(es) => {
                for e in es {
                    e.regions(out);
                }
            }
            RTy::Option(e) | RTy::Vec(e) | RTy::Set(e) | RTy::Cell(e) => e.regions(out),
            RTy::Result(a, b) | RTy::Map(a, b) => {
                a.regions(out);
                b.regions(out);
            }
            RTy::Iter { region, elem, .. } => {
                out.extend(region.iter().copied());
                elem.regions(out);
            }
            RTy::Closure { region, ret } => {
                out.extend(region.iter().copied());
                ret.regions(out);
            }
            RTy::String | RTy::Str | RTy::Opaque(_) => {}
        }
    }

    pub fn has_regions(&self) -> bool {
        let mut v = Vec::new();
        self.regions(&mut v);
        !v.is_empty()
    }

    /// The same shape with every region replaced by `f(region)`.
    pub fn map_regions(&self, f: &mut dyn FnMut(Region) -> Region) -> RTy {
        match self {
            RTy::Ref(r, m, inner) => RTy::Ref(f(*r), *m, Box::new(inner.map_regions(f))),
            RTy::Adt(n, args, rs) => RTy::Adt(
                n.clone(),
                args.iter().map(|a| a.map_regions(f)).collect(),
                rs.iter().map(|r| f(*r)).collect(),
            ),
            RTy::Tuple(es) => RTy::Tuple(es.iter().map(|e| e.map_regions(f)).collect()),
            RTy::Option(e) => RTy::Option(Box::new(e.map_regions(f))),
            RTy::Vec(e) => RTy::Vec(Box::new(e.map_regions(f))),
            RTy::Set(e) => RTy::Set(Box::new(e.map_regions(f))),
            RTy::Cell(e) => RTy::Cell(Box::new(e.map_regions(f))),
            RTy::Result(a, b) => {
                RTy::Result(Box::new(a.map_regions(f)), Box::new(b.map_regions(f)))
            }
            RTy::Map(a, b) => RTy::Map(Box::new(a.map_regions(f)), Box::new(b.map_regions(f))),
            RTy::Iter {
                region,
                mutable,
                elem,
            } => RTy::Iter {
                region: region.map(&mut *f),
                mutable: *mutable,
                elem: Box::new(elem.map_regions(f)),
            },
            RTy::Closure { region, ret } => RTy::Closure {
                region: region.map(&mut *f),
                ret: Box::new(ret.map_regions(f)),
            },
            other => other.clone(),
        }
    }

    /// The type behind every `&` (`&&str` → `str`).
    pub fn strip_refs(&self) -> &RTy {
        match self {
            RTy::Ref(_, _, inner) => inner.strip_refs(),
            other => other,
        }
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, RTy::Ref(..))
    }
}

/// A readable listing of a body, for debugging (`NLL_DUMP=1`, native builds).
#[allow(dead_code)]
pub fn dump(b: &Body) -> String {
    let mut s = format!("fn {} ({} points)\n", b.name, b.point_count());
    for (i, l) in b.locals.iter().enumerate() {
        let _ = writeln!(
            s,
            "  _{i} {:?} `{}`: {:?}{}",
            l.kind,
            l.name,
            l.rty,
            if l.drops { " drops" } else { "" }
        );
    }
    for (i, r) in b.regions.iter().enumerate() {
        if let Some(u) = &r.universal {
            let _ = writeln!(s, "  '{i} universal {} outlives {:?}", u.name, u.outlives);
        }
    }
    for c in &b.constraints {
        let _ = writeln!(s, "  '{}: '{} @{:?}", c.longer.0, c.shorter.0, c.span);
    }
    let place = |p: &Place| {
        let mut t = format!("_{}", p.local.0);
        for pr in &p.proj {
            match pr {
                Proj::Field(f) => t = format!("{t}.{f}"),
                Proj::Deref => t = format!("(*{t})"),
                Proj::Index => t = format!("{t}[_]"),
                Proj::Downcast(v) => t = format!("({t} as {v})"),
            }
        }
        t
    };
    let op = |o: &Operand| format!("{}{}", if o.moves { "move " } else { "" }, place(&o.place));
    let mut p = 0;
    for (bi, blk) in b.blocks.iter().enumerate() {
        let _ = writeln!(s, "  bb{bi}:");
        for st in &blk.stmts {
            let text = match &st.kind {
                StmtKind::Assign(d, rv) => {
                    let r = match rv {
                        Rvalue::Use(o) => op(o),
                        Rvalue::Ref {
                            region,
                            mutable,
                            place: pl,
                            two_phase,
                        } => format!(
                            "&'{}{} {}{}",
                            region.0,
                            if *mutable { " mut" } else { "" },
                            place(pl),
                            if *two_phase { " (two-phase)" } else { "" }
                        ),
                        Rvalue::Ops { args, activate } => format!(
                            "ops({}){}",
                            args.iter().map(op).collect::<Vec<_>>().join(", "),
                            if activate.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    " activate {}",
                                    activate
                                        .iter()
                                        .map(|(pl, r)| format!("{}@'{}", place(pl), r.0))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            }
                        ),
                    };
                    format!("{} = {r}", place(d))
                }
                StmtKind::StorageDead(l) => format!("StorageDead(_{})", l.0),
            };
            let _ = writeln!(s, "    {p}: {text} @{:?}", st.span);
            p += 1;
        }
        let term = match &blk.term {
            Term::Goto(t) => format!("goto bb{}", t.0),
            Term::Switch(o, ts) => format!(
                "switch {} -> {:?}",
                o.as_ref().map(op).unwrap_or_default(),
                ts.iter().map(|t| t.0).collect::<Vec<_>>()
            ),
            Term::Return => "return".to_string(),
            Term::Unreachable => "unreachable".to_string(),
        };
        let _ = writeln!(s, "    {p}: {term}");
        p += 1;
    }
    s
}
