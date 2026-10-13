// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Calculating again the cells a change affects.
//!
//! After cells change, the formulas that depend on them (directly, or
//! through other formulas, or through defined names) are calculated again,
//! each after the cells it refers to. Cells no change reaches keep the
//! values Excel last calculated: the engine never calculates more than it
//! must, so a function it does not have yet changes nothing that did not
//! change.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::eval::{Area, Array, Book, Cells, Ctx};
use crate::formula::{Expr, parse};
use crate::value::{CellError, CellValue};

/// A cell of the book: sheet, row, column.
pub type CellId = (usize, u32, u32);

/// A cell calculated again.
#[derive(Clone, Debug, PartialEq)]
pub struct Recalculated {
    pub cell: CellId,
    pub value: CellValue,
    /// The formula used a function the engine does not have (or a random
    /// or time function): the value is only a stand-in.
    pub unsupported: bool,
    /// A dynamic array formula's result of more than one value: rows,
    /// columns and the values row by row (the first is `value`), to spill.
    pub spill: Option<(usize, usize, Vec<CellValue>)>,
}

/// What a formula refers to, for finding what depends on what.
#[derive(Clone, Debug, Default)]
pub struct Dependencies {
    /// Rectangles (sheet, first row, first column, last row, last column).
    pub areas: Vec<(usize, usize, u32, u32, u32, u32)>,
    /// Depends on everything (volatile functions, INDIRECT, OFFSET, names
    /// that could not be followed).
    pub volatile: bool,
}

fn dependencies(book: &dyn Book, sheet: usize, expr: &Expr, out: &mut Dependencies, depth: u32) {
    let resolve = |s: &Option<crate::formula::SheetRef>| -> Option<(usize, usize)> {
        match s {
            None => Some((sheet, sheet)),
            Some(r) if r.book.is_some() => None,
            Some(r) => {
                let a = book.sheet_index(&r.first)?;
                let b = r.last.as_ref().map_or(Some(a), |l| book.sheet_index(l))?;
                Some((a.min(b), a.max(b)))
            }
        }
    };
    match expr {
        Expr::Ref(r) => {
            if let Some((a, b)) = resolve(&r.sheet) {
                out.areas.push((a, b, r.start.row, r.start.column, r.end.row, r.end.column));
            }
        }
        Expr::Name { sheet: s, name } => {
            let scope = match s {
                Some(r) => book.sheet_index(&r.first),
                None => Some(sheet),
            };
            match book.defined_name(name, scope) {
                Some(f) if depth < 32 => match parse(f.trim_start_matches('=')) {
                    Ok(e) => dependencies(book, scope.unwrap_or(sheet), &e, out, depth + 1),
                    Err(_) => {}
                },
                Some(_) => out.volatile = true,
                // An unknown name (maybe bound by LET): nothing to follow.
                None => {}
            }
        }
        Expr::Call { name, args } => {
            if matches!(name.as_str(), "INDIRECT" | "OFFSET" | "NOW" | "TODAY" | "RAND" | "RANDBETWEEN" | "RANDARRAY" | "CELL" | "INFO") {
                out.volatile = true;
            }
            if crate::functions::lookup(name).is_none() && crate::functions::table::function_info(name).is_none() {
                // A defined name holding a LAMBDA.
                dependencies(book, sheet, &Expr::Name { sheet: None, name: name.clone() }, out, depth);
            }
            for a in args {
                dependencies(book, sheet, a, out, depth);
            }
        }
        Expr::Invoke { callee, args } => {
            dependencies(book, sheet, callee, out, depth);
            for a in args {
                dependencies(book, sheet, a, out, depth);
            }
        }
        Expr::Array(rows) => rows.iter().flatten().for_each(|e| dependencies(book, sheet, e, out, depth)),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Implicit(x) => dependencies(book, sheet, x, out, depth),
        Expr::Spill(inner) => {
            out.volatile = true;
            dependencies(book, sheet, inner, out, depth);
        }
        Expr::Structured { .. } => out.volatile = true,
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::Intersect(a, b) => {
            dependencies(book, sheet, a, out, depth);
            dependencies(book, sheet, b, out, depth);
        }
        Expr::Union(items) => items.iter().for_each(|e| dependencies(book, sheet, e, out, depth)),
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing => {}
    }
}

/// A book's formulas, parsed once for a recalculation.
struct Formulas {
    parsed: HashMap<CellId, Option<Arc<Expr>>>,
}

/// The cells being calculated again, and their new values.
struct Recalc<'b> {
    dirty: HashSet<CellId>,
    formulas: RefCell<Formulas>,
    results: RefCell<HashMap<CellId, (CellValue, bool)>>,
    spills: RefCell<HashMap<CellId, (usize, usize, Vec<CellValue>)>>,
    in_progress: RefCell<HashSet<CellId>>,
    iter_values: RefCell<HashMap<CellId, CellValue>>,
    cache: RefCell<HashMap<String, Option<Arc<Expr>>>>,
    /// Ranges made into arrays, with the spill version they were made at.
    arrays: RefCell<HashMap<Area, (u64, Rc<Array>)>>,
    spill_version: std::cell::Cell<u64>,
    _book: std::marker::PhantomData<&'b ()>,
}

impl Recalc<'_> {
    fn expr(&self, book: &dyn Book, cell: CellId) -> Option<Arc<Expr>> {
        if let Some(e) = self.formulas.borrow().parsed.get(&cell) {
            return e.clone();
        }
        let text = book.formula(cell.0, cell.1, cell.2)?.to_string();
        let e = self.cache.borrow_mut().entry(text.clone()).or_insert_with(|| parse(&text).ok().map(Arc::new)).clone();
        self.formulas.borrow_mut().parsed.insert(cell, e.clone());
        e
    }

    fn calculate_eval(&self, book: &dyn Book, cell: CellId) -> (CellValue, bool) {
        match self.expr(book, cell) {
            None if book.formula(cell.0, cell.1, cell.2).is_some() => (CellValue::Error(CellError::Name), false),
            None => (book.value(cell.0, cell.1, cell.2), false),
            Some(expr) => {
                self.in_progress.borrow_mut().insert(cell);
                let mut ctx = Ctx::new(book, self, cell.0, cell.1, cell.2);
                let v = ctx.eval(&expr);
                let is_dynamic = book.dynamic(cell.0, cell.1, cell.2);
                if is_dynamic
                    && let Some(spill) = ctx.spill_result(&v)
                {
                    self.spills.borrow_mut().insert(cell, spill);
                    self.spill_version.set(self.spill_version.get() + 1);
                }
                let value = ctx.cell_result(v);
                let unsupported = ctx.unsupported;
                self.in_progress.borrow_mut().remove(&cell);
                (value, unsupported)
            }
        }
    }

    fn calculate(&self, book: &dyn Book, cell: CellId) -> (CellValue, bool) {
        if let Some(r) = self.results.borrow().get(&cell) {
            return r.clone();
        }
        let result = self.calculate_eval(book, cell);
        self.results.borrow_mut().insert(cell, result.clone());
        result
    }
}

impl Cells for Recalc<'_> {
    fn fresh(&self, book: &dyn Book, sheet: usize, row: u32, column: u32) -> Option<CellValue> {
        let cell = (sheet, row, column);
        if let Some(v) = self.iter_values.borrow().get(&cell) {
            return Some(v.clone());
        }
        if !self.dirty.contains(&cell) {
            for (&(s, ar, ac), (rows, cols, values)) in self.spills.borrow().iter() {
                if s == sheet && row >= ar && row < ar + *rows as u32 && column >= ac && column < ac + *cols as u32 {
                    let idx = ((row - ar) as usize) * cols + ((column - ac) as usize);
                    if let Some(v) = values.get(idx) {
                        return Some(v.clone());
                    }
                }
            }
            return None;
        }
        if let Some(r) = self.results.borrow().get(&cell) {
            if r.1 {
                return Some(book.value(sheet, row, column));
            }
            return Some(r.0.clone());
        }
        if self.in_progress.borrow().contains(&cell) {
            // A circular reference: Excel shows 0 (and warns).
            return Some(CellValue::Number(0.0));
        }
        let (value, unsupported) = self.calculate(book, cell);
        if unsupported {
            // Keep what Excel calculated for a formula we cannot.
            return Some(book.value(sheet, row, column));
        }
        Some(value)
    }

    fn cached_array(&self, area: &Area) -> Option<Rc<Array>> {
        if !self.iter_values.borrow().is_empty() {
            return None;
        }
        let arrays = self.arrays.borrow();
        let (version, array) = arrays.get(area)?;
        (*version == self.spill_version.get()).then(|| array.clone())
    }

    fn store_array(&self, area: &Area, array: &Rc<Array>) {
        // Only when every cell is final: not while a cell in the range is
        // being calculated (a circular reference) or while iterating.
        if !self.iter_values.borrow().is_empty() {
            return;
        }
        let busy = self.in_progress.borrow().iter().any(|&(s, r, c)| {
            s >= area.sheet && s <= area.last_sheet && r >= area.first_row && r <= area.last_row && c >= area.first_column && c <= area.last_column
        });
        if !busy {
            self.arrays.borrow_mut().insert(*area, (self.spill_version.get(), array.clone()));
        }
    }

    fn spill_area(&self, sheet: usize, row: u32, column: u32) -> Option<(u32, u32, u32, u32)> {
        let cell = (sheet, row, column);
        if let Some((rows, cols, _)) = self.spills.borrow().get(&cell) {
            return Some((row, column, row + *rows as u32 - 1, column + *cols as u32 - 1));
        }
        None
    }

    fn for_each_cell(&self, sheet: usize, (r0, c0, r1, c1): (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
        let results = self.results.borrow();
        // A small area is looked up cell by cell; scanning all the results
        // for each SUM of a few cells made a big workbook's calculation
        // take minutes (the results are as many as the formulas).
        let cells = (r1.saturating_sub(r0) as u64 + 1) * (c1.saturating_sub(c0) as u64 + 1);
        if cells <= results.len() as u64 {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    if let Some((v, _)) = results.get(&(sheet, r, c)) {
                        f(r, c, v);
                    }
                }
            }
        } else {
            for (&(s, r, c), (v, _)) in results.iter() {
                if s == sheet && r >= r0 && r <= r1 && c >= c0 && c <= c1 {
                    f(r, c, v);
                }
            }
        }
        drop(results);
        for (&(s, ar, ac), (rows, cols, values)) in self.spills.borrow().iter() {
            if s == sheet {
                for dr in 0..*rows as u32 {
                    for dc in 0..*cols as u32 {
                        let r = ar + dr;
                        let c = ac + dc;
                        if r >= r0 && r <= r1 && c >= c0 && c <= c1 {
                            let idx = (dr as usize) * cols + (dc as usize);
                            if let Some(v) = values.get(idx) {
                                f(r, c, v);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn find_sccs(nodes: &[CellId], adj: &HashMap<CellId, Vec<CellId>>) -> Vec<Vec<CellId>> {
    struct State<'a> {
        adj: &'a HashMap<CellId, Vec<CellId>>,
        index: usize,
        indices: HashMap<CellId, usize>,
        lowlink: HashMap<CellId, usize>,
        on_stack: HashSet<CellId>,
        stack: Vec<CellId>,
        sccs: Vec<Vec<CellId>>,
    }

    fn strongconnect(u: CellId, st: &mut State) {
        st.indices.insert(u, st.index);
        st.lowlink.insert(u, st.index);
        st.index += 1;
        st.stack.push(u);
        st.on_stack.insert(u);

        if let Some(neighbors) = st.adj.get(&u) {
            for &v in neighbors {
                if !st.indices.contains_key(&v) {
                    strongconnect(v, st);
                    let v_low = st.lowlink[&v];
                    let u_low = st.lowlink.get_mut(&u).unwrap();
                    *u_low = (*u_low).min(v_low);
                } else if st.on_stack.contains(&v) {
                    let v_idx = st.indices[&v];
                    let u_low = st.lowlink.get_mut(&u).unwrap();
                    *u_low = (*u_low).min(v_idx);
                }
            }
        }

        if st.lowlink[&u] == st.indices[&u] {
            let mut scc = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack.remove(&w);
                scc.push(w);
                if w == u {
                    break;
                }
            }
            st.sccs.push(scc);
        }
    }

    let mut state = State {
        adj,
        index: 0,
        indices: HashMap::new(),
        lowlink: HashMap::new(),
        on_stack: HashSet::new(),
        stack: Vec::new(),
        sccs: Vec::new(),
    };

    for &node in nodes {
        if !state.indices.contains_key(&node) {
            strongconnect(node, &mut state);
        }
    }

    state.sccs
}

/// Precomputed dependencies per referenced sheet.
#[derive(Clone, Debug, Default)]
pub struct SheetDependencies {
    /// Formulas referencing wide column ranges (> 32 cols) or the whole sheet
    pub wide_formulas: Vec<CellId>,
    /// Formulas referencing specific column: col -> Vec<CellId>
    pub by_col: HashMap<u32, Vec<CellId>>,
}

/// Precomputed dependencies and parsed ASTs for workbook formulas,
/// allowing sub-millisecond recalculation on cell edits.
#[derive(Clone, Debug, Default)]
pub struct DependencyIndex {
    /// Formula expressions per cell: CellId -> Option<Arc<Expr>>
    pub parsed: HashMap<CellId, Option<Arc<Expr>>>,
    /// Dependencies per cell: CellId -> Dependencies
    pub deps: HashMap<CellId, Dependencies>,
    /// List of volatile formula cells
    pub volatile: Vec<CellId>,
    /// For each sheet index S: dependencies touching sheet S
    pub sheets: Vec<SheetDependencies>,
    /// AST cache by formula string
    pub ast_cache: HashMap<String, Option<Arc<Expr>>>,
}

/// The largest sheet index among `cells` (an area's sheet range is cut to it).
fn sheet_count_hint(cells: &std::collections::BTreeSet<CellId>) -> usize {
    cells.iter().next_back().map_or(0, |c| c.0)
}

impl DependencyIndex {
    /// Build index from all formula cells of the book.
    pub fn build(book: &dyn Book, all_formulas: &[CellId]) -> Self {
        Self::build_with_cache(book, all_formulas, HashMap::new())
    }

    /// The parsed formulas by their text, to give to [`DependencyIndex::build_with_cache`]
    /// of the book after a change (what did not change is not parsed again).
    pub fn take_cache(&mut self) -> HashMap<String, Option<Arc<Expr>>> {
        std::mem::take(&mut self.ast_cache)
    }

    /// `build`, with formulas parsed earlier (`previous`, from `take_cache`).
    pub fn build_with_cache(book: &dyn Book, all_formulas: &[CellId], previous: HashMap<String, Option<Arc<Expr>>>) -> Self {
        let sheet_count = book.sheet_count();
        let mut parsed = HashMap::with_capacity(all_formulas.len());
        let mut deps = HashMap::with_capacity(all_formulas.len());
        let mut volatile = Vec::new();
        let mut sheets: Vec<SheetDependencies> = (0..sheet_count).map(|_| SheetDependencies::default()).collect();
        let mut ast_cache: HashMap<String, Option<Arc<Expr>>> = HashMap::new();

        for &cell in all_formulas {
            let Some(text) = book.formula(cell.0, cell.1, cell.2) else { continue };
            let expr = match ast_cache.get(text) {
                Some(e) => e.clone(),
                None => {
                    let e = previous.get(text).cloned().unwrap_or_else(|| parse(text).ok().map(Arc::new));
                    ast_cache.insert(text.to_string(), e.clone());
                    e
                }
            };
            let mut d = Dependencies::default();
            if let Some(e) = &expr {
                dependencies(book, cell.0, e, &mut d, 0);
            }
            if d.volatile {
                volatile.push(cell);
            }
            for &(s0, s1, _r0, c0, _r1, c1) in &d.areas {
                for s in s0..=s1 {
                    if s < sheet_count {
                        let col_count = c1.saturating_sub(c0) + 1;
                        if col_count > 32 {
                            sheets[s].wide_formulas.push(cell);
                        } else {
                            for c in c0..=c1 {
                                sheets[s].by_col.entry(c).or_default().push(cell);
                            }
                        }
                    }
                }
            }
            parsed.insert(cell, expr);
            deps.insert(cell, d);
        }

        Self {
            parsed,
            deps,
            volatile,
            sheets,
            ast_cache,
        }
    }

    /// Update or add a formula cell in the index.
    pub fn update_formula(&mut self, book: &dyn Book, cell: CellId) {
        self.remove_formula(&cell);
        let Some(text) = book.formula(cell.0, cell.1, cell.2) else { return };
        let expr = self.ast_cache.entry(text.to_string()).or_insert_with(|| parse(text).ok().map(Arc::new)).clone();
        let mut d = Dependencies::default();
        if let Some(e) = &expr {
            dependencies(book, cell.0, e, &mut d, 0);
        }
        if d.volatile {
            self.volatile.push(cell);
        }
        let sheet_count = book.sheet_count();
        if self.sheets.len() < sheet_count {
            self.sheets.resize_with(sheet_count, SheetDependencies::default);
        }
        for &(s0, s1, _r0, c0, _r1, c1) in &d.areas {
            for s in s0..=s1 {
                if s < self.sheets.len() {
                    let col_count = c1.saturating_sub(c0) + 1;
                    if col_count > 32 {
                        self.sheets[s].wide_formulas.push(cell);
                    } else {
                        for c in c0..=c1 {
                            self.sheets[s].by_col.entry(c).or_default().push(cell);
                        }
                    }
                }
            }
        }
        self.parsed.insert(cell, expr);
        self.deps.insert(cell, d);
    }

    /// Remove a formula cell from the index.
    pub fn remove_formula(&mut self, cell: &CellId) {
        if let Some(d) = self.deps.remove(cell) {
            self.parsed.remove(cell);
            self.volatile.retain(|c| c != cell);
            for &(s0, s1, _r0, c0, _r1, c1) in &d.areas {
                for s in s0..=s1 {
                    if s < self.sheets.len() {
                        let col_count = c1.saturating_sub(c0) + 1;
                        if col_count > 32 {
                            self.sheets[s].wide_formulas.retain(|c| c != cell);
                        } else {
                            for c in c0..=c1 {
                                if let Some(cols) = self.sheets[s].by_col.get_mut(&c) {
                                    cols.retain(|c| c != cell);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Identify dirty formula cells affected by `changed` cells.
    pub fn find_dirty(&self, changed: &[CellId]) -> HashSet<CellId> {
        let mut dirty: HashSet<CellId> = changed.iter().copied().filter(|c| self.parsed.contains_key(c)).collect();
        let mut frontier: Vec<CellId> = changed.to_vec();
        let mut first_round = true;

        while !frontier.is_empty() {
            let mut next = Vec::new();
            // The changed cells by (sheet, column, row), to ask whether any lies in an area.
            let by_column: std::collections::BTreeSet<(usize, u32, u32)> = frontier.iter().map(|&(s, r, c)| (s, c, r)).collect();

            let mut candidate_cells: Vec<CellId> = Vec::new();
            if first_round {
                candidate_cells.extend_from_slice(&self.volatile);
            }

            // What refers to a column is listed once per column, not once per
            // changed cell (a sort changes tens of thousands of cells).
            let mut sheets_seen: HashSet<usize> = HashSet::new();
            let mut columns_seen: HashSet<(usize, u32)> = HashSet::new();
            for &(s, _r, c) in &frontier {
                if s < self.sheets.len() {
                    let s_deps = &self.sheets[s];
                    if sheets_seen.insert(s) {
                        candidate_cells.extend_from_slice(&s_deps.wide_formulas);
                    }
                    if columns_seen.insert((s, c))
                        && let Some(cols) = s_deps.by_col.get(&c)
                    {
                        candidate_cells.extend_from_slice(cols);
                    }
                }
            }

            for cell in candidate_cells {
                if dirty.contains(&cell) {
                    continue;
                }
                let Some(d) = self.deps.get(&cell) else { continue };
                let hit = (first_round && d.volatile)
                    || d.areas.iter().any(|&(s0, s1, r0, c0, r1, c1)| {
                        if (c1 - c0) < 64 && (s1 - s0) < 64 {
                            (s0..=s1).any(|s| (c0..=c1).any(|c| by_column.range((s, c, r0)..=(s, c, r1)).next().is_some()))
                        } else {
                            frontier.iter().any(|&(s, r, c)| (s0..=s1).contains(&s) && (r0..=r1).contains(&r) && (c0..=c1).contains(&c))
                        }
                    });
                if hit {
                    dirty.insert(cell);
                    next.push(cell);
                }
            }
            first_round = false;
            frontier = next;
        }

        dirty
    }

    /// Calculate again what depends on `changed` cells using the precomputed index.
    pub fn recalculate(&self, book: &dyn Book, changed: &[CellId]) -> Vec<Recalculated> {
        let dirty = self.find_dirty(changed);
        self.recalculate_set(book, dirty)
    }

    /// Calculate exactly `cells` (each after the cells it refers to), not what
    /// depends on them: for cells whose formulas have no value yet, the rest
    /// of the book being as the file has it.
    pub fn calculate_only(&self, book: &dyn Book, cells: &[CellId]) -> Vec<Recalculated> {
        let dirty: HashSet<CellId> = cells.iter().copied().filter(|c| self.parsed.contains_key(c)).collect();
        self.recalculate_set(book, dirty)
    }

    fn recalculate_set(&self, book: &dyn Book, dirty: HashSet<CellId>) -> Vec<Recalculated> {
        if dirty.is_empty() {
            return Vec::new();
        }

        let recalc = Recalc {
            dirty: dirty.clone(),
            formulas: RefCell::new(Formulas { parsed: self.parsed.clone() }),
            results: RefCell::new(HashMap::new()),
            spills: RefCell::new(HashMap::new()),
            in_progress: RefCell::new(HashSet::new()),
            iter_values: RefCell::new(HashMap::new()),
            cache: RefCell::new(self.ast_cache.clone()),
            arrays: RefCell::new(HashMap::new()),
            spill_version: std::cell::Cell::new(0),
            _book: std::marker::PhantomData,
        };

        let mut dirty_cells: Vec<CellId> = dirty.iter().copied().collect();
        dirty_cells.sort();

        // Construct adjacency graph among dirty cells only
        // (Each area looks up the dirty cells inside it in an ordered index, rather
        // than testing every dirty cell: a workbook of tens of thousands of formulas
        // took quadratic time.)
        let ordered: std::collections::BTreeSet<CellId> = dirty_cells.iter().copied().collect();
        let mut adj: HashMap<CellId, Vec<CellId>> = HashMap::with_capacity(dirty_cells.len());
        for &cell in &dirty_cells {
            let mut neighbors: Vec<CellId> = Vec::new();
            if let Some(d) = self.deps.get(&cell) {
                for &(s0, s1, r0, c0, r1, c1) in &d.areas {
                    for sheet in s0..=s1.min(sheet_count_hint(&ordered)) {
                        for &other in ordered.range((sheet, r0, 0)..=(sheet, r1, u32::MAX)) {
                            if (c0..=c1).contains(&other.2) {
                                neighbors.push(other);
                            }
                        }
                    }
                }
                neighbors.sort();
                neighbors.dedup();
            }
            adj.insert(cell, neighbors);
        }

        let sccs = find_sccs(&dirty_cells, &adj);

        for mut scc in sccs {
            scc.sort();
            let is_cycle = scc.len() > 1 || (scc.len() == 1 && adj.get(&scc[0]).map_or(false, |nbrs| nbrs.contains(&scc[0])));

            if !is_cycle {
                let cell = scc[0];
                recalc.calculate(book, cell);
            } else if !book.iterate() {
                // Circular reference with iterative calculation disabled: Excel displays 0.
                for &cell in &scc {
                    recalc.results.borrow_mut().insert(cell, (CellValue::Number(0.0), false));
                }
            } else {
                // Circular reference with iterative calculation enabled: Gauss-Seidel iterations
                for &cell in &scc {
                    let initial = match book.value(cell.0, cell.1, cell.2) {
                        CellValue::Number(n) => CellValue::Number(n),
                        _ => CellValue::Number(0.0),
                    };
                    recalc.iter_values.borrow_mut().insert(cell, initial);
                }

                let max_iters = book.max_iterations().max(1);
                let max_change = book.max_change();

                for _ in 0..max_iters {
                    let mut max_diff: f64 = 0.0;
                    for &cell in &scc {
                        let (new_val, unsupported) = recalc.calculate_eval(book, cell);

                        let old_num = match recalc.iter_values.borrow().get(&cell) {
                            Some(CellValue::Number(n)) => Some(*n),
                            _ => None,
                        };
                        let new_num = match &new_val {
                            CellValue::Number(n) => Some(*n),
                            _ => None,
                        };
                        if let (Some(o), Some(n)) = (old_num, new_num) {
                            let diff = (n - o).abs();
                            if diff > max_diff {
                                max_diff = diff;
                            }
                        } else if old_num != new_num {
                            max_diff = f64::INFINITY;
                        }

                        recalc.iter_values.borrow_mut().insert(cell, new_val.clone());
                        recalc.results.borrow_mut().insert(cell, (new_val, unsupported));
                    }

                    if max_diff <= max_change {
                        break;
                    }
                }
            }
        }

        let mut cells: Vec<CellId> = recalc.dirty.iter().copied().collect();
        cells.sort();
        cells
            .into_iter()
            .map(|cell| {
                let (value, unsupported) = recalc.calculate(book, cell);
                let spill = recalc.spills.borrow().get(&cell).cloned();
                Recalculated { cell, value, unsupported, spill }
            })
            .collect()
    }
}

/// Calculate again what depends on the `changed` cells (whose new values
/// or formulas are already in the book), and the changed cells that have
/// formulas. `all_formulas` lists every formula cell of the book.
pub fn recalculate(book: &dyn Book, changed: &[CellId], all_formulas: &[CellId]) -> Vec<Recalculated> {
    let index = DependencyIndex::build(book, all_formulas);
    index.recalculate(book, changed)
}

/// Calculate one formula for a cell as things stand (no recalculation of
/// what it refers to), for checking the engine against Excel's values.
pub fn evaluate(book: &dyn Book, cell: CellId, formula: &str) -> (CellValue, bool) {
    match parse(formula) {
        Ok(expr) => {
            let mut ctx = Ctx::new(book, &crate::eval::Stored, cell.0, cell.1, cell.2);
            let v = ctx.eval(&expr);
            let value = ctx.cell_result(v);
            (value, ctx.unsupported)
        }
        Err(_) => (CellValue::Error(CellError::Name), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBook {
        sheets: Vec<String>,
        formulas: HashMap<CellId, String>,
        values: HashMap<CellId, CellValue>,
        iterate: bool,
        max_iterations: u32,
        max_change: f64,
        dynamic: bool,
    }

    impl MockBook {
        fn new(iterate: bool) -> Self {
            Self {
                sheets: vec!["Sheet1".to_string()],
                formulas: HashMap::new(),
                values: HashMap::new(),
                iterate,
                max_iterations: 100,
                max_change: 0.001,
                dynamic: false,
            }
        }
    }

    impl Book for MockBook {
        fn sheet_count(&self) -> usize {
            self.sheets.len()
        }
        fn sheet_name(&self, sheet: usize) -> &str {
            self.sheets.get(sheet).map(|s| s.as_str()).unwrap_or("")
        }
        fn value(&self, sheet: usize, row: u32, column: u32) -> CellValue {
            self.values.get(&(sheet, row, column)).cloned().unwrap_or(CellValue::Empty)
        }
        fn formula(&self, sheet: usize, row: u32, column: u32) -> Option<&str> {
            self.formulas.get(&(sheet, row, column)).map(|s| s.as_str())
        }
        fn for_each_cell(&self, sheet: usize, (r0, c0, r1, c1): (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    let v = self.value(sheet, r, c);
                    if !v.is_empty() {
                        f(r, c, &v);
                    }
                }
            }
        }
        fn extent(&self, _sheet: usize) -> (u32, u32) {
            (100, 100)
        }
        fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> {
            None
        }
        fn iterate(&self) -> bool {
            self.iterate
        }
        fn max_iterations(&self) -> u32 {
            self.max_iterations
        }
        fn max_change(&self) -> f64 {
            self.max_change
        }
        fn dynamic(&self, _sheet: usize, _row: u32, _column: u32) -> bool {
            self.dynamic
        }
    }

    #[test]
    fn a_lookup_table_shared_within_a_recalculation_is_not_stale_after_an_edit() {
        let mut book = MockBook::new(false);
        // A1:A100 keys 1..100, B1:B100 = A * 2 (formulas, so the table is made of results).
        let mut formulas = Vec::new();
        for r in 0..100u32 {
            book.values.insert((0, r, 0), CellValue::Number((r + 1) as f64));
            book.formulas.insert((0, r, 1), format!("A{}*2", r + 1));
            formulas.push((0, r, 1));
        }
        // D1..D100 look up the same range cell by cell; E1 goes through INDIRECT, F1 through OFFSET.
        for r in 0..100u32 {
            book.formulas.insert((0, r, 3), format!("VLOOKUP(A{},$A$1:$B$100,2,FALSE)", r + 1));
            formulas.push((0, r, 3));
        }
        book.formulas.insert((0, 0, 4), "VLOOKUP(5,INDIRECT(\"A1:B100\"),2,FALSE)".to_string());
        book.formulas.insert((0, 0, 5), "VLOOKUP(5,OFFSET(A1,0,0,100,2),2,FALSE)".to_string());
        formulas.push((0, 0, 4));
        formulas.push((0, 0, 5));
        let get = |res: &[Recalculated], cell: (usize, u32, u32)| res.iter().find(|r| r.cell == cell).map(|r| r.value.clone()).unwrap();

        let all = recalculate(&book, &formulas, &formulas);
        assert_eq!(get(&all, (0, 4, 3)), CellValue::Number(10.0));
        assert_eq!(get(&all, (0, 0, 4)), CellValue::Number(10.0));
        // The cached values are what the book holds now.
        for r in &all {
            book.values.insert(r.cell, r.value.clone());
        }

        // Change A5 (the key of row 5 becomes 500): only what depends on it is calculated again,
        // and the lookups must see the new range, not the table made before.
        book.values.insert((0, 4, 0), CellValue::Number(500.0));
        let part = recalculate(&book, &[(0, 4, 0)], &formulas);
        assert_eq!(get(&part, (0, 4, 1)), CellValue::Number(1000.0));
        assert_eq!(get(&part, (0, 4, 3)), CellValue::Number(1000.0));
        assert_eq!(get(&part, (0, 0, 4)), CellValue::Error(CellError::NA));
        assert_eq!(get(&part, (0, 0, 5)), CellValue::Error(CellError::NA));
    }

    #[test]
    fn test_circular_without_iterative() {
        let mut book = MockBook::new(false);
        // B1 (0, 0, 1) = B1 + 1
        book.formulas.insert((0, 0, 1), "B1+1".to_string());
        let res = recalculate(&book, &[(0, 0, 1)], &[(0, 0, 1)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value, CellValue::Number(0.0));
    }

    #[test]
    fn test_iterative_self_addition() {
        let mut book = MockBook::new(true);
        // B1 (0, 0, 1) = B1 + 1 (100 iterations)
        book.formulas.insert((0, 0, 1), "B1+1".to_string());
        let res = recalculate(&book, &[(0, 0, 1)], &[(0, 0, 1)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value, CellValue::Number(100.0));
    }

    #[test]
    fn test_iterative_if_counter() {
        let mut book = MockBook::new(true);
        // B3 (0, 2, 1) = IF(B3<10, B3+1, B3)
        book.formulas.insert((0, 2, 1), "IF(B3<10, B3+1, B3)".to_string());
        let res = recalculate(&book, &[(0, 2, 1)], &[(0, 2, 1)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value, CellValue::Number(10.0));
    }

    #[test]
    fn test_iterative_linear_system_and_downstream() {
        let mut book = MockBook::new(true);
        // B5 (0, 4, 1) = (100-C5)/2
        // C5 (0, 4, 2) = (80-B5)/3
        // D5 (0, 4, 3) = B5 + C5  (downstream non-cyclic formula)
        book.formulas.insert((0, 4, 1), "(100-C5)/2".to_string());
        book.formulas.insert((0, 4, 2), "(80-B5)/3".to_string());
        book.formulas.insert((0, 4, 3), "B5+C5".to_string());

        let formulas = vec![(0, 4, 1), (0, 4, 2), (0, 4, 3)];
        let res = recalculate(&book, &formulas, &formulas);
        assert_eq!(res.len(), 3);

        let b5_val = res.iter().find(|r| r.cell == (0, 4, 1)).unwrap().value.as_number().unwrap();
        let c5_val = res.iter().find(|r| r.cell == (0, 4, 2)).unwrap().value.as_number().unwrap();
        let d5_val = res.iter().find(|r| r.cell == (0, 4, 3)).unwrap().value.as_number().unwrap();

        // Exact analytical solution:
        // 2*B5 + C5 = 100
        // B5 + 3*C5 = 80
        // 6*B5 + 3*C5 = 300 => 5*B5 = 220 => B5 = 44, C5 = 12
        assert!((b5_val - 44.0).abs() < 0.01, "B5 was {}", b5_val);
        assert!((c5_val - 12.0).abs() < 0.01, "C5 was {}", c5_val);
        assert!((d5_val - 56.0).abs() < 0.01, "D5 was {}", d5_val);
    }

    #[test]
    fn test_array_constants() {
        let mut book = MockBook::new(false);
        book.dynamic = true;
        // Direct spill: A1 (0, 0, 0) = {1, 2; 3, 4}
        book.formulas.insert((0, 0, 0), "{1, 2; 3, 4}".to_string());
        // SUM with array constant: C1 (0, 0, 2) = SUM({1, 2; 3, 4})
        book.formulas.insert((0, 0, 2), "SUM({1, 2; 3, 4})".to_string());
        // AVERAGE with 1D array constant: C2 (0, 1, 2) = AVERAGE({2, 4, 6})
        book.formulas.insert((0, 1, 2), "AVERAGE({2, 4, 6})".to_string());
        // INDEX with array constant: C3 (0, 2, 2) = INDEX({10, 20; 30, 40}, 2, 1)
        book.formulas.insert((0, 2, 2), "INDEX({10, 20; 30, 40}, 2, 1)".to_string());
        // VLOOKUP with array constant: C4 (0, 3, 2) = VLOOKUP(2, {1, "A"; 2, "B"; 3, "C"}, 2, FALSE)
        book.formulas.insert((0, 3, 2), "VLOOKUP(2, {1, \"A\"; 2, \"B\"; 3, \"C\"}, 2, FALSE)".to_string());
        // Array arithmetic broadcast: C5 (0, 4, 2) = {1, 2} + 10 (spill 1x2)
        book.formulas.insert((0, 4, 2), "{1, 2} + 10".to_string());

        let cells = vec![(0, 0, 0), (0, 0, 2), (0, 1, 2), (0, 2, 2), (0, 3, 2), (0, 4, 2)];
        let res = recalculate(&book, &cells, &cells);

        // A1 spills 2x2
        let a1 = res.iter().find(|r| r.cell == (0, 0, 0)).unwrap();
        assert_eq!(a1.value, CellValue::Number(1.0));
        assert_eq!(a1.spill, Some((2, 2, vec![CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Number(3.0), CellValue::Number(4.0)])));

        // C1 = SUM({1, 2; 3, 4}) = 10
        let c1 = res.iter().find(|r| r.cell == (0, 0, 2)).unwrap();
        assert_eq!(c1.value, CellValue::Number(10.0));
        assert_eq!(c1.spill, None);

        // C2 = AVERAGE({2, 4, 6}) = 4
        let c2 = res.iter().find(|r| r.cell == (0, 1, 2)).unwrap();
        assert_eq!(c2.value, CellValue::Number(4.0));
        assert_eq!(c2.spill, None);

        // C3 = INDEX({10, 20; 30, 40}, 2, 1) = 30
        let c3 = res.iter().find(|r| r.cell == (0, 2, 2)).unwrap();
        assert_eq!(c3.value, CellValue::Number(30.0));

        // C4 = VLOOKUP(2, {1, "A"; 2, "B"; 3, "C"}, 2, FALSE) = "B"
        let c4 = res.iter().find(|r| r.cell == (0, 3, 2)).unwrap();
        assert_eq!(c4.value, CellValue::Text("B".to_string()));

        // C5 = {1, 2} + 10 spills 1x2: [11, 12]
        let c5 = res.iter().find(|r| r.cell == (0, 4, 2)).unwrap();
        assert_eq!(c5.value, CellValue::Number(11.0));
        assert_eq!(c5.spill, Some((1, 2, vec![CellValue::Number(11.0), CellValue::Number(12.0)])));

        // Mixed types: C6 (0, 5, 2) = {"hello", TRUE; 100, #N/A}
        book.formulas.insert((0, 5, 2), "{\"hello\", TRUE; 100, #N/A}".to_string());
        let res2 = recalculate(&book, &[(0, 5, 2)], &[(0, 5, 2)]);
        let c6 = res2.iter().find(|r| r.cell == (0, 5, 2)).unwrap();
        assert_eq!(c6.value, CellValue::Text("hello".to_string()));
        assert_eq!(c6.spill, Some((2, 2, vec![
            CellValue::Text("hello".to_string()),
            CellValue::Bool(true),
            CellValue::Number(100.0),
            CellValue::Error(CellError::NA),
        ])));

        // Syntax error: unequal row widths
        assert!(parse("{1, 2; 3}").is_err());
        assert!(parse("{1, 2; 3, 4, 5}").is_err());
    }

    #[test]
    fn test_3d_references() {
        let mut book = MockBook::new(false);
        book.sheets = vec!["Sheet1".to_string(), "Sheet2".to_string(), "Sheet3".to_string()];

        // Values across 3 sheets:
        // Sheet1 (0): A1=10, B1=5
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 0, 1), CellValue::Number(5.0));

        // Sheet2 (1): A1=20, B1=15
        book.values.insert((1, 0, 0), CellValue::Number(20.0));
        book.values.insert((1, 0, 1), CellValue::Number(15.0));

        // Sheet3 (2): A1=30, B1=25
        book.values.insert((2, 0, 0), CellValue::Number(30.0));
        book.values.insert((2, 0, 1), CellValue::Number(25.0));

        // Formulas in Sheet1:
        // C1 (0, 0, 2) = SUM(Sheet1:Sheet3!A1) -> 60
        book.formulas.insert((0, 0, 2), "SUM(Sheet1:Sheet3!A1)".to_string());
        // C2 (0, 1, 2) = AVERAGE(Sheet1:Sheet3!A1) -> 20
        book.formulas.insert((0, 1, 2), "AVERAGE(Sheet1:Sheet3!A1)".to_string());
        // C3 (0, 2, 2) = COUNT(Sheet1:Sheet3!A1) -> 3
        book.formulas.insert((0, 2, 2), "COUNT(Sheet1:Sheet3!A1)".to_string());
        // C4 (0, 3, 2) = MAX(Sheet1:Sheet3!A1) -> 30
        book.formulas.insert((0, 3, 2), "MAX(Sheet1:Sheet3!A1)".to_string());
        // C5 (0, 4, 2) = MIN(Sheet1:Sheet3!A1) -> 10
        book.formulas.insert((0, 4, 2), "MIN(Sheet1:Sheet3!A1)".to_string());
        // C6 (0, 5, 2) = SUM(Sheet1:Sheet3!A1:B1) -> 105
        book.formulas.insert((0, 5, 2), "SUM(Sheet1:Sheet3!A1:B1)".to_string());
        // C7 (0, 6, 2) = SUM(Sheet3:Sheet1!A1) -> 60 (reverse sheet order)
        book.formulas.insert((0, 6, 2), "SUM(Sheet3:Sheet1!A1)".to_string());

        let formulas = vec![
            (0, 0, 2),
            (0, 1, 2),
            (0, 2, 2),
            (0, 3, 2),
            (0, 4, 2),
            (0, 5, 2),
            (0, 6, 2),
        ];

        let res = recalculate(&book, &formulas, &formulas);
        assert_eq!(res.iter().find(|r| r.cell == (0, 0, 2)).unwrap().value, CellValue::Number(60.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 1, 2)).unwrap().value, CellValue::Number(20.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 2, 2)).unwrap().value, CellValue::Number(3.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 3, 2)).unwrap().value, CellValue::Number(30.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 4, 2)).unwrap().value, CellValue::Number(10.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 5, 2)).unwrap().value, CellValue::Number(105.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 6, 2)).unwrap().value, CellValue::Number(60.0));

        // Cascade recalculation test:
        // Change Sheet2 A1 from 20 to 50
        book.values.insert((1, 0, 0), CellValue::Number(50.0));
        let changed = vec![(1, 0, 0)];
        let res2 = recalculate(&book, &changed, &formulas);
        let c1_new = res2.iter().find(|r| r.cell == (0, 0, 2)).unwrap();
        assert_eq!(c1_new.value, CellValue::Number(90.0), "C1 recalculated from 60 to 90");
    }

    #[test]
    fn test_operator_precedence() {
        let mut book = MockBook::new(false);

        // Cells for reference-based calculations
        // A1 (0, 0, 0) = 10, B1 (0, 0, 1) = 2, C1 (0, 0, 2) = 3
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 0, 1), CellValue::Number(2.0));
        book.values.insert((0, 0, 2), CellValue::Number(3.0));

        let cases = vec![
            // 1. Basic arithmetic and parentheses
            ((0, 1, 0), "1+2*3", CellValue::Number(7.0)),
            ((0, 1, 1), "(1+2)*3", CellValue::Number(9.0)),
            ((0, 1, 2), "10-4/2", CellValue::Number(8.0)),
            ((0, 1, 3), "16/4/2", CellValue::Number(2.0)),
            ((0, 1, 4), "10-5-2", CellValue::Number(3.0)),

            // 2. Power and unary minus
            ((0, 2, 0), "2*3^2", CellValue::Number(18.0)),
            ((0, 2, 1), "(2*3)^2", CellValue::Number(36.0)),
            ((0, 2, 2), "2^3^2", CellValue::Number(64.0)), // left-associative in Excel: (2^3)^2 = 64
            ((0, 2, 3), "-2^2", CellValue::Number(4.0)),   // Excel unary minus binds tighter than power: (-2)^2 = 4
            ((0, 2, 4), "-(2^2)", CellValue::Number(-4.0)),
            ((0, 2, 5), "10-2^3", CellValue::Number(2.0)),

            // 3. Percent operator
            ((0, 3, 0), "50%*200", CellValue::Number(100.0)),
            ((0, 3, 1), "10+50%", CellValue::Number(10.5)),
            ((0, 3, 2), "20%+30%", CellValue::Number(0.5)),

            // 4. String concatenation
            ((0, 4, 0), "1+2&3+4", CellValue::Text("37".into())),
            ((0, 4, 1), "\"Val:\"&10*2", CellValue::Text("Val:20".into())),

            // 5. Comparison operators
            ((0, 5, 0), "1+2=3", CellValue::Bool(true)),
            ((0, 5, 1), "1+2*3=7", CellValue::Bool(true)),
            ((0, 5, 2), "1+2*3=9", CellValue::Bool(false)),
            ((0, 5, 3), "1&2=\"12\"", CellValue::Bool(true)),
            ((0, 5, 4), "1+2<2*2", CellValue::Bool(true)),
            ((0, 5, 5), "5>=2+3", CellValue::Bool(true)),
            ((0, 5, 6), "10<>5*2", CellValue::Bool(false)),
            ((0, 5, 7), "1<2=TRUE", CellValue::Bool(true)),

            // 6. Cell references with operator precedence
            ((0, 6, 0), "A1+B1*C1", CellValue::Number(16.0)),
            ((0, 6, 1), "(A1+B1)*C1", CellValue::Number(36.0)),
            ((0, 6, 2), "-B1^2", CellValue::Number(4.0)), // (-2)^2 = 4
            ((0, 6, 3), "A1&B1", CellValue::Text("102".into())),
            ((0, 6, 4), "A1>B1*C1", CellValue::Bool(true)), // 10 > 6 = true
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in &cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in &cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_implicit_intersection_operator() {
        let mut book = MockBook::new(false);
        book.dynamic = true; // Dynamic array environment

        // Data cells:
        // Column range A1:A5 (rows 0..=4, column 0)
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 1, 0), CellValue::Number(20.0));
        book.values.insert((0, 2, 0), CellValue::Number(30.0));
        book.values.insert((0, 3, 0), CellValue::Number(40.0));
        book.values.insert((0, 4, 0), CellValue::Number(50.0));

        // Row range C1:E1 (row 0, columns 2..=4)
        book.values.insert((0, 0, 2), CellValue::Number(100.0));
        book.values.insert((0, 0, 3), CellValue::Number(200.0));
        book.values.insert((0, 0, 4), CellValue::Number(300.0));

        let cases = vec![
            // 1. Column range intersection by row:
            ((0, 0, 1), "@A1:A5", CellValue::Number(10.0)), // row 0 intersects with A1
            ((0, 1, 1), "@A1:A5", CellValue::Number(20.0)), // row 1 intersects with A2
            ((0, 2, 1), "@A1:A5", CellValue::Number(30.0)), // row 2 intersects with A3
            ((0, 4, 1), "@A1:A5", CellValue::Number(50.0)), // row 4 intersects with A5
            ((0, 9, 1), "@A1:A5", CellValue::Error(CellError::Value)), // row 9 does not intersect -> #VALUE!

            // 2. Row range intersection by column:
            ((0, 2, 2), "@C1:E1", CellValue::Number(100.0)), // col 2 intersects with C1
            ((0, 2, 3), "@C1:E1", CellValue::Number(200.0)), // col 3 intersects with D1
            ((0, 2, 4), "@C1:E1", CellValue::Number(300.0)), // col 4 intersects with E1
            ((0, 2, 5), "@C1:E1", CellValue::Error(CellError::Value)), // col 5 does not intersect -> #VALUE!

            // 3. Array constants and dynamic formulas (suppress spill, extract first element):
            ((0, 0, 6), "@{1000, 2000; 3000, 4000}", CellValue::Number(1000.0)),
            ((0, 1, 6), "@SEQUENCE(5)", CellValue::Number(1.0)),

            // 4. Combined with arithmetic:
            ((0, 1, 7), "10+@A1:A5", CellValue::Number(30.0)), // 10 + 20 (row 1) = 30
            ((0, 2, 7), "@A1:A5*2", CellValue::Number(60.0)),  // 30 * 2 (row 2) = 60
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in &cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in &cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }

        // 5. Cascade recalculation: change A2 from 20 to 99
        book.values.insert((0, 1, 0), CellValue::Number(99.0));
        let changed = vec![(0, 1, 0)];
        let res2 = recalculate(&book, &changed, &formula_cells);

        // B2 (row 1, col 1) should update to 99
        let b2 = res2.iter().find(|r| r.cell == (0, 1, 1)).unwrap();
        assert_eq!(b2.value, CellValue::Number(99.0), "B2 recalculated to 99");

        // H2 (row 1, col 7) = 10+@A1:A5 should update to 109
        let h2 = res2.iter().find(|r| r.cell == (0, 1, 7)).unwrap();
        assert_eq!(h2.value, CellValue::Number(109.0), "H2 recalculated to 109");
    }

    #[test]
    fn test_text_delimiter_functions() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Fractional match_mode and match_end truncated
            ((0, 0, 0), "TEXTBEFORE(\"aB|ab\", \"ab\", 1, 0.5, 0, \"missing\")", CellValue::Text("aB|".into())),
            ((0, 0, 1), "TEXTBEFORE(\"abc\", \"x\", 1, 0, 0.5, \"missing\")", CellValue::Text("missing".into())),
            ((0, 0, 2), "TEXTAFTER(\"aB|ab\", \"ab\", 1, 0.5, 0, \"missing\")", CellValue::Text("".into())),
            ((0, 0, 3), "TEXTAFTER(\"abc\", \"x\", 1, 0, 0.5, \"missing\")", CellValue::Text("missing".into())),

            // 2. Negative instance out of bounds -> #VALUE!
            ((0, 1, 0), "TEXTBEFORE(\"abc\", \"x\", -1000, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 1, 1), "TEXTBEFORE(\"abc\", \"x\", -4, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 1, 2), "TEXTAFTER(\"abc\", \"x\", -1000, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 1, 3), "TEXTAFTER(\"abc\", \"x\", -4, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),

            // 3. Empty text -> #VALUE!
            ((0, 2, 0), "TEXTBEFORE(\"\", \"x\", 1, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 2, 1), "TEXTAFTER(\"\", \"x\", 1, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),

            // 4. Empty delimiter
            ((0, 3, 0), "TEXTBEFORE(\"abc\", \"\", -2, 0, 0, \"missing\")", CellValue::Text("abc".into())),
            ((0, 3, 1), "TEXTBEFORE(\"abc\", \"\", 2, 0, 0, \"missing\")", CellValue::Text("".into())),
            ((0, 3, 2), "TEXTAFTER(\"abc\", \"\", -2, 0, 0, \"missing\")", CellValue::Text("".into())),
            ((0, 3, 3), "TEXTAFTER(\"abc\", \"\", 2, 0, 0, \"missing\")", CellValue::Text("abc".into())),

            // 5. Fallback as string type
            ((0, 4, 0), "TEXTBEFORE(\"abc\", \"x\", 1, 0, 0, TRUE)", CellValue::Text("TRUE".into())),
            ((0, 4, 1), "TEXTBEFORE(\"abc\", \"x\", 1, 0, 0, 0)", CellValue::Text("0".into())),
            ((0, 4, 2), "TEXTAFTER(\"abc\", \"x\", 1, 0, 0, TRUE)", CellValue::Text("TRUE".into())),

            // 6. TEXTSPLIT edge cases
            ((0, 5, 0), "TEXTSPLIT(\"\", \",\")", CellValue::Error(CellError::Value)),
            ((0, 5, 1), "TEXTSPLIT(\"Hello World\", \"Hello World\", \"Hello World\", TRUE)", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_char_and_code_functions() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 0), "CHAR(255)", CellValue::Text("ÿ".into())),
            ((0, 0, 1), "CHAR(3)", CellValue::Text("\u{3}".into())),
            ((0, 0, 2), "CHAR(10)", CellValue::Text("\n".into())),
            ((0, 0, 3), "CHAR(0)", CellValue::Error(CellError::Value)),
            ((0, 0, 4), "CHAR(256)", CellValue::Error(CellError::Value)),
            ((0, 1, 0), "CODE(\"日本語テキスト\")", CellValue::Number(18044.0)),
            ((0, 1, 1), "CODE(\"abc\")", CellValue::Number(97.0)),
            ((0, 1, 2), "CODE(\"ｱｲｳ ABC\")", CellValue::Number(177.0)),
            ((0, 1, 3), "CODE(\"\")", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_acot_acoth_and_type_functions() {
        let mut book = MockBook::new(false);
        book.values.insert((0, 0, 0), CellValue::Number(1.0)); // A1 = 1
        book.values.insert((0, 1, 0), CellValue::Number(2.0)); // A2 = 2

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 1), "ACOT(10)", CellValue::Number(0.09966865249116204)),
            ((0, 0, 2), "ACOT(100)", CellValue::Number(0.009999666686665238)),
            ((0, 0, 3), "ACOT(1E15)", CellValue::Number(1e-15)),
            ((0, 0, 4), "ACOT(-1)", CellValue::Number(std::f64::consts::PI * 0.75)),
            ((0, 1, 1), "ACOTH(100)", CellValue::Number(0.010000333353334763)),
            ((0, 1, 2), "ACOTH(1E15)", CellValue::Number(1e-15)),
            ((0, 1, 3), "ACOTH(1)", CellValue::Error(CellError::Num)),
            ((0, 1, 4), "ACOTH(0.5)", CellValue::Error(CellError::Num)),
            // TYPE: array constant is 64
            ((0, 2, 0), "TYPE({1, 2; 3, 4})", CellValue::Number(64.0)),
            // TYPE: single cell reference is cell type (1 for number)
            ((0, 0, 5), "TYPE(A1)", CellValue::Number(1.0)),
            // TYPE: out-of-intersection range reference is #VALUE! -> type 16
            ((0, 10, 0), "TYPE(A1:A2)", CellValue::Number(16.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_spill_hash_and_anchorarray() {
        let mut book = MockBook::new(false);
        book.dynamic = true;
        book.formulas.insert((0, 0, 1), "SEQUENCE(4, 1, 10, 5)".to_string());
        book.formulas.insert((0, 0, 3), "SUM(B1#)".to_string());
        book.formulas.insert((0, 1, 3), "INDEX(B1#, 2)".to_string());
        book.formulas.insert((0, 0, 4), "SUM(ANCHORARRAY(B1))".to_string());
        book.formulas.insert((0, 1, 4), "INDEX(ANCHORARRAY(B1), 2)".to_string());

        let formula_cells = vec![(0, 0, 1), (0, 0, 3), (0, 1, 3), (0, 0, 4), (0, 1, 4)];
        let res = recalculate(&book, &formula_cells, &formula_cells);
        let val = |cell| res.iter().find(|r| r.cell == cell).map(|r| r.value.clone()).unwrap();
        assert_eq!(val((0, 0, 1)), CellValue::Number(10.0));
        assert_eq!(val((0, 0, 3)), CellValue::Number(70.0));
        assert_eq!(val((0, 1, 3)), CellValue::Number(15.0));
        assert_eq!(val((0, 0, 4)), CellValue::Number(70.0));
        assert_eq!(val((0, 1, 4)), CellValue::Number(15.0));
    }

    #[test]
    fn test_n_and_t_range_functions() {
        let mut book = MockBook::new(false);
        // Set up cells in sheet 0
        // A1: 10
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        // A2: "hello"
        book.values.insert((0, 1, 0), CellValue::Text("hello".to_string()));
        // A3: TRUE
        book.values.insert((0, 2, 0), CellValue::Bool(true));
        // A4: Empty (implicit)
        // A5: #DIV/0!
        book.values.insert((0, 4, 0), CellValue::Error(CellError::Div0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // N function: evaluates top-left cell of range
            ((0, 10, 0), "N(A1:A5)", CellValue::Number(10.0)),
            ((0, 10, 1), "N(A2:A5)", CellValue::Number(0.0)),
            ((0, 10, 2), "N(A3)", CellValue::Number(1.0)),
            ((0, 10, 3), "N(A4)", CellValue::Number(0.0)),
            ((0, 10, 4), "N(A5)", CellValue::Error(CellError::Div0)),
            ((0, 10, 5), "N({42, 2; 3, 4})", CellValue::Number(42.0)),
            // T function: evaluates top-left cell of range
            ((0, 11, 0), "T(A1:A5)", CellValue::Text("".to_string())),
            ((0, 11, 1), "T(A2:A5)", CellValue::Text("hello".to_string())),
            ((0, 11, 2), "T(A3)", CellValue::Text("".to_string())),
            ((0, 11, 3), "T(A4)", CellValue::Text("".to_string())),
            ((0, 11, 4), "T(A5)", CellValue::Error(CellError::Div0)),
            ((0, 11, 5), "T({1, 2; 3, 4})", CellValue::Text("".to_string())),
            ((0, 11, 6), "T({\"world\", 2; 3, 4})", CellValue::Text("world".to_string())),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_factdouble_boolean_error() {
        let mut book = MockBook::new(false);
        book.values.insert((0, 0, 0), CellValue::Bool(true));
        book.values.insert((0, 1, 0), CellValue::Bool(false));
        book.values.insert((0, 2, 0), CellValue::Number(6.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 10, 0), "FACTDOUBLE(6)", CellValue::Number(48.0)),
            ((0, 10, 1), "FACTDOUBLE(7)", CellValue::Number(105.0)),
            ((0, 10, 2), "FACTDOUBLE(0)", CellValue::Number(1.0)),
            ((0, 10, 3), "FACTDOUBLE(-1)", CellValue::Number(1.0)),
            ((0, 10, 4), "FACTDOUBLE(-2)", CellValue::Error(CellError::Num)),
            ((0, 10, 5), "FACTDOUBLE(\"2\")", CellValue::Number(2.0)),
            ((0, 10, 6), "FACTDOUBLE(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 10, 7), "FACTDOUBLE(FALSE)", CellValue::Error(CellError::Value)),
            ((0, 10, 8), "FACTDOUBLE(A1)", CellValue::Error(CellError::Value)),
            ((0, 10, 9), "FACTDOUBLE(A2)", CellValue::Error(CellError::Value)),
            ((0, 10, 10), "FACTDOUBLE(A3)", CellValue::Number(48.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_imlog2_precision() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 20, 0), "IMLOG2(\"12:30\")", CellValue::Text("-0.941106310946432".to_string())),
            ((0, 20, 1), "IMLOG2(123)", CellValue::Text("6.94251450533924".to_string())),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_dist_boolean_argument() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 30, 0), "EXPON.DIST(100, 0.5, \"TRUE\")", CellValue::Number(1.0)),
            ((0, 30, 1), "NORM.S.DIST(1000, \"TRUE\")", CellValue::Number(1.0)),
            ((0, 30, 2), "EXPON.DIST(0, 0.5, \"FALSE\")", CellValue::Number(0.5)),
            ((0, 30, 3), "EXPON.DIST(100, 0.5, \"x\")", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_error_type_calc_filter() {
        let mut book = MockBook::new(false);
        book.dynamic = true;
        book.values.insert((0, 0, 0), CellValue::Number(1.0)); // A1
        book.values.insert((0, 1, 0), CellValue::Number(3.0)); // A2

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 44, 1), "ERROR.TYPE(_xlfn._xlws.FILTER(A1:A2, A1:A2>100))", CellValue::Number(14.0)),
            ((0, 44, 2), "ERROR.TYPE(1/0)", CellValue::Number(2.0)),
            ((0, 44, 3), "ERROR.TYPE(NA())", CellValue::Number(7.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_fisherinv_precision() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 0), "FISHERINV(1E-15)", CellValue::Number(9.992007221626401e-16)),
            ((0, 0, 1), "FISHERINV(0)", CellValue::Number(0.0)),
            ((0, 0, 2), "FISHERINV(100)", CellValue::Number(1.0)),
            ((0, 0, 3), "FISHERINV(1E15)", CellValue::Number(1.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_gammaln_precision() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 0), "GAMMALN(2.5)", CellValue::Number(0.2846828704729194)),
            ((0, 0, 1), "_xlfn.GAMMALN.PRECISE(2.5)", CellValue::Number(0.2846828704729194)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            if let (CellValue::Number(act), CellValue::Number(exp)) = (actual, expected) {
                assert!((act - exp).abs() < 1e-15, "formula `{formula}` precision mismatch: {act} vs {exp}");
            } else {
                panic!("expected number");
            }
        }
    }

    #[test]
    fn test_intercept_precision() {
        let mut book = MockBook::new(false);
        // D!A1:B5 and D!J1:J10 simulation
        // A1:A5 = 1, 2, 3, 4, 5
        // B1:B5 = -2.5, 0, 3.7, 12, -7
        // J1:J10 = 2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3
        for i in 0..5 {
            book.values.insert((0, i, 0), CellValue::Number((i + 1) as f64)); // A1..A5
        }
        let b_vals = [-2.5, 0.0, 3.7, 12.0, -7.0, 100.0, 0.25, 8.0, 1e5, 42.0];
        for (i, &v) in b_vals.iter().enumerate() {
            book.values.insert((0, i as u32, 1), CellValue::Number(v)); // B1..B10
        }
        let j_vals = [2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3];
        for (i, &v) in j_vals.iter().enumerate() {
            book.values.insert((0, i as u32, 9), CellValue::Number(v)); // J1..J10
        }

        let cases: &[((usize, u32, u32), &str, f64)] = &[
            ((0, 0, 20), "INTERCEPT(A1:B5, J1:J10)", 0.7248121627076085),
            ((0, 1, 20), "_xlfn.FORECAST.LINEAR(2.5, J1:J10, B1:B10)", 10.27591956519375),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            if let CellValue::Number(act) = actual {
                assert!((act - expected).abs() < 1e-15, "formula `{formula}` precision mismatch: {act} vs {expected}");
            } else {
                panic!("expected number for {formula}");
            }
        }
    }

    #[test]
    fn test_math_and_stat_excel_compliance() {
        let mut book = MockBook::new(false);
        // L1 is empty (default in MockBook)
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 50, 0), "ASINH(1E-15)", CellValue::Number(1.1102230246251559e-15)),
            ((0, 50, 1), "_xlfn.SECH(10)", CellValue::Number(9.0799859337817237e-5)),
            ((0, 50, 2), "_xlfn.BASE(0, 10, 0)", CellValue::Text(String::new())),
            ((0, 50, 3), "_xlfn.BASE(0, 2)", CellValue::Text("0".to_string())),
            ((0, 50, 4), "MOD(1E+15, 100)", CellValue::Error(CellError::Num)),
            ((0, 50, 5), "QUOTIENT(TRUE, 0.5)", CellValue::Error(CellError::Value)),
            ((0, 50, 6), "QUOTIENT(1000, FALSE)", CellValue::Error(CellError::Value)),
            ((0, 50, 7), "QUOTIENT(10, 3)", CellValue::Number(3.0)),
            ((0, 50, 8), "AVEDEV(L1)", CellValue::Error(CellError::Num)),
            ((0, 50, 9), "POWER(100, -2.5)", CellValue::Number(9.99999999999998e-6)),
            ((0, 50, 10), "POWER(0.05, 100)", CellValue::Number(7.888609052210203e-131)),
            ((0, 50, 11), "POWER(0.05, 2.5)", CellValue::Number(0.0005590169943749478)),
            ((0, 50, 12), "POWER({1,2,3}, -2.5)", CellValue::Number(1.0)),
            ((0, 50, 13), "SQRTPI(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 14), "MROUND(TRUE, 5)", CellValue::Error(CellError::Value)),
            ((0, 50, 15), "GCD(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 16), "GCD(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 17), "LCM(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 18), "LCM(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 19), "_xlfn.PERCENTOF(\"abc\", 10)", CellValue::Error(CellError::Value)),
            ((0, 50, 20), "MULTINOMIAL(2, 2, 2)", CellValue::Number(89.999999999999915)),
            ((0, 50, 21), "MULTINOMIAL(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 22), "MULTINOMIAL(255)", CellValue::Error(CellError::Num)),
            ((0, 50, 23), "DATEDIF(DATE(2020,1,31),DATE(2021,3,1),\"MD\")", CellValue::Number(-2.0)),
            ((0, 50, 24), "DATEVALUE(\"令和6年1月15日\")", CellValue::Number(45306.0)),
            ((0, 50, 25), "DATEVALUE(\"R6.1.15\")", CellValue::Number(45306.0)),
            ((0, 50, 26), "DATEVALUE(\"H31.4.30\")", CellValue::Number(43585.0)),
            ((0, 50, 27), "DATEVALUE(\"Jan 15, 2024\")", CellValue::Number(45306.0)),
            ((0, 50, 28), "DATEVALUE(\"1900/2/29\")", CellValue::Number(60.0)),
            ((0, 50, 29), "DATEVALUE(\"明治45年7月30日\")", CellValue::Number(4595.0)),
            ((0, 50, 30), "DATEVALUE(\"大正元年7月30日\")", CellValue::Error(CellError::Value)),
            ((0, 50, 31), "SUBTOTAL(9, Sheet1:Sheet2!A1:A3)", CellValue::Error(CellError::Value)),
            ((0, 50, 32), "_xlfn.AGGREGATE(14, 6, A1:A3*2, 1)", CellValue::Number(60.0)),
            ((0, 50, 33), "_xlfn.AGGREGATE(15, 6, A1:A3*2, 1)", CellValue::Number(20.0)),
            ((0, 50, 34), "DEVSQ(L1)", CellValue::Error(CellError::Num)),
            ((0, 50, 35), "HARMEAN(L1)", CellValue::Error(CellError::NA)),
            ((0, 50, 36), "SMALL({10, 20, 30}, 2.9)", CellValue::Number(20.0)),
            ((0, 50, 37), "_xlfn.MAXIFS(A1:A3, C1:C3, \"=\")", CellValue::Number(0.0)),
            // exp(-5e-16): Apple's libm gives Excel's value, glibc the correctly rounded one, a unit in the last place below.
            ((0, 50, 38), "_xlfn.CHISQ.DIST.RT(1E-15, 2)", CellValue::Number(if cfg!(target_os = "linux") { 0.99999999999999944 } else { 0.99999999999999956 })),
            ((0, 50, 39), "SUBSTITUTE(\"123\",\"12:30\",\"A1\",TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 40), "DBCS(\"ｶﾞﾊﾟｳﾞ\")", CellValue::Text("ガパウ゛".to_string())),
            ((0, 50, 41), "LARGE({10, 20, 30}, 2.9)", CellValue::Number(10.0)),
            ((0, 50, 42), "_xlfn.SWITCH({1,2;3,4}, 1, \"one\", 2, \"two\", \"default\")", CellValue::Text("default".to_string())),
            ((0, 50, 43), "COUPDAYSNC(DATE(2021,1,1), DATE(2021,2,28), 2, 4)", CellValue::Number(57.0)),
            ((0, 50, 44), "COUPDAYBS(0, DATE(2021,2,28), 2, 0)", CellValue::Number(0.0)),
            ((0, 50, 45), "_xlfn.TRIMRANGE(L1:L10)", CellValue::Error(CellError::Ref)),
            ((0, 50, 46), "LOOKUP(1, \"abc\")", CellValue::Error(CellError::NA)),
            ((0, 50, 47), "MATCH(2, {1, 3, 2}, 1)", CellValue::Number(1.0)),
            ((0, 50, 48), "_xlfn.ARRAYTOTEXT(_xlfn.DROP(F1:F3, 2, 0), 1)", CellValue::Error(CellError::Value)),
            ((0, 50, 49), "_xlfn.ARRAYTOTEXT(_xlfn.ANCHORARRAY(F1), 1)", CellValue::Error(CellError::NA)),
            ((0, 50, 50), "_xlfn.ARRAYTOTEXT(_xlfn.EXPAND(A1:A3, 3, 2, L1), 1)", CellValue::Text("{10,;20,;30,}".to_string())),
            ((0, 50, 51), "ISPMT(10, 3, 2.9, 0.25)", CellValue::Number(0.08620689655172421)),
            ((0, 50, 52), "NOMINAL(123456789, 5)", CellValue::Number(202.62182322772847)),
            ((0, 50, 53), "PV(3.7, 2, 10, 0.5, 0)", CellValue::Number(-2.602987777274785)),
            ((0, 50, 54), "_xlfn.ARRAYTOTEXT(_xlfn.SCAN(A1:A3, A1:A3, LAMBDA(a, v, a+v)), 1)", CellValue::Text("{#VALUE!;#VALUE!;#VALUE!}".to_string())),
            ((0, 50, 55), "LEFTB(\"日本語テキスト\", 3)", CellValue::Text("日 ".to_string())),
            ((0, 50, 56), "RIGHTB(\"日本語テキスト\", 10)", CellValue::Text("語テキスト".to_string())),
            ((0, 50, 57), "MIDB(\"日本語テキスト\", 2, 3)", CellValue::Text(" 本".to_string())),
            ((0, 50, 58), "REPLACEB(\"日本語テキスト\", 10, 5, \"Hello World\")", CellValue::Text("日本語テ Hello World".to_string())),
            ((0, 50, 59), "LOGINV(1E-15, 3.7, 0.5)", CellValue::Number(0.76286616979737698)),
            ((0, 50, 60), "DISC(-1, 47119, 2, 3.7)", CellValue::Error(CellError::Num)),
            ((0, 50, 61), "INTRATE(-1, 47119, 10, 100)", CellValue::Error(CellError::Num)),
            ((0, 50, 62), "PRICEDISC(-1, 44197, 3.7, 10)", CellValue::Error(CellError::Num)),
            ((0, 50, 63), "YIELDDISC(-1, 47119, 100, 0.25)", CellValue::Error(CellError::Num)),
            ((0, 50, 64), "AMORLINC(1, -1, 46000, 1, 1, 0.25)", CellValue::Error(CellError::Num)),
            ((0, 50, 65), "ACCRINTM(45000, 45000.75, 0.25, 10)", CellValue::Number(0.0)),
            ((0, 50, 66), "ACCRINTM(-1, 46000, 0.25, 10)", CellValue::Error(CellError::Num)),
            ((0, 50, 67), "AND(\"x\", TRUE, TRUE)", CellValue::Bool(true)),
            ((0, 50, 68), "OR(\"x\", TRUE, TRUE)", CellValue::Bool(true)),
            ((0, 50, 69), "XOR(\"x\", TRUE, TRUE)", CellValue::Bool(false)),
            ((0, 50, 70), "AND(\"x\")", CellValue::Error(CellError::Value)),
            ((0, 50, 71), "TEXT(1, \"Banana\")", CellValue::Error(CellError::Value)),
            ((0, 50, 72), "TEXT(1, \"ABC-def_ghi\")", CellValue::Text("A43C-133f 0i".to_string())),
            ((0, 50, 73), "EXPONDIST(1E-15, 0.25, TRUE)", CellValue::Number(2.4999999999999997e-16)),
            ((0, 50, 74), "BINOMDIST(0, 1, 1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 75), "BINOMDIST(1, 3, 1, FALSE)", CellValue::Number(0.0)),
            ((0, 50, 76), "GAMMA(0.5)", CellValue::Number(std::f64::consts::PI.sqrt())),
            ((0, 50, 77), "MAKEARRAY(2, 2, LAMBDA(r, c, {1, 2}))", CellValue::Error(CellError::Value)),
            ((0, 50, 78), "VAR(\"5\", 3.7, 3.7)", CellValue::Number(0.56333333333333613)),
            ((0, 50, 79), "VAR(3.7, 3.7, 3.7)", CellValue::Number(2.9582283945787943e-31)),
            ((0, 50, 80), "_xlfn.VAR.P(2.5, 3.7, 3.7)", CellValue::Number(0.32000000000000106)),
            ((0, 50, 81), "VARP(\"5\", 3.7, 3.7)", CellValue::Number(0.3755555555555582)),
            ((0, 50, 82), "_xlfn.STDEV.P(2.5, 3.7, 3.7)", CellValue::Number(0.56568542494923901)),
            ((0, 50, 83), "STDEVP(3.7, 2.5, 3.7)", CellValue::Number(0.56568542494923901)),
            ((0, 50, 84), "_xlfn.NORM.DIST(-1, 10, 5, FALSE)", CellValue::Number(7.0949185692462842e-3)),
            ((0, 50, 85), "COUPNUM(0, 47119, 4)", CellValue::Error(CellError::Num)),
            ((0, 50, 86), "COUPNUM(1, 47119, 4)", CellValue::Number(516.0)),
            ((0, 50, 87), "COUPPCD(1, 45000, 2)", CellValue::Number(0.0)),
            ((0, 50, 88), "TBILLEQ(39538, 39600, 10)", CellValue::Error(CellError::Num)),
            ((0, 50, 89), "XNPV(0, A1:A3, A1:A3)", CellValue::Error(CellError::Num)),
            ((0, 50, 90), "XNPV(0.05, \"abc\", A1:A3)", CellValue::Error(CellError::Value)),
            ((0, 50, 91), "PMT(1E-15, 5, 12, -2.5, 0)", CellValue::Number(-1.9000000000000081)),
            ((0, 50, 92), "PMT(0.05, \"2\", 1)", CellValue::Number(-0.53780487804878041)),
            ((0, 50, 93), "FV(-2.5, 0, 10, 0.25, 0)", CellValue::Number(-0.25)),
            ((0, 50, 94), "FV(2, 2.9, 0.05)", CellValue::Number(-0.57977196039251466)),
            ((0, 50, 95), "IPMT(-1, 1, 1, 10, 0.25, 1)", CellValue::Number(0.0)),
            ((0, 50, 96), "_xlfn.NUMBERVALUE(\"2024/1/31\", \"a,b,c\", \"abc\")", CellValue::Error(CellError::Value)),
            ((0, 50, 97), "_xlfn.NUMBERVALUE(\"2024/1/31\", \"a,b,c\", \"\")", CellValue::Error(CellError::Value)),
            ((0, 50, 98), "_xlfn.NUMBERVALUE(\"2024/1/31\", \"a,b,c\", \"123\")", CellValue::Error(CellError::Value)),
            ((0, 50, 99), "GEOMEAN(1E+15)", CellValue::Number(1e15)),
            ((0, 50, 100), "GEOMEAN(2.5, 3.7, 3.7)", CellValue::Number(3.2467422977985749)),
            ((0, 50, 101), "_xlfn.XMATCH(\"a.c\", {\"abc\", \"a.c\"}, 3, 1)", CellValue::Number(1.0)),
            ((0, 50, 102), "_xlfn.XMATCH(\"a.c\", {\"abc\", \"a.c\"}, 3, -1)", CellValue::Number(2.0)),
            ((0, 50, 103), "_xlfn.XMATCH(35, {10, 20, 30, 40, 50, 60, 70, 80, 90, 100}, -1, -2)", CellValue::Error(CellError::NA)),
            ((0, 50, 104), "IPMT(3.7, 2, 2, 3.7, 3.7)", CellValue::Number(-8.8864912280701773)),
            ((0, 50, 105), "EFFECT(1, 255)", CellValue::Number(1.712970952642269)),
            ((0, 50, 106), "EFFECT(1, 1000)", CellValue::Number(1.7169239322355203)),
            ((0, 50, 107), "DB(123456789, 3.7, 10, 5)", CellValue::Number(99725.880122494535)),
            ((0, 50, 108), "DB(10, 1E-15, 10, 5)", CellValue::Number(3.8085937500000032e-6)),
            ((0, 50, 109), "INFO(\"origin\")", CellValue::Text("$A:$A$1".into())),
            ((0, 50, 110), "CONFIDENCE(0.25, 100, 3)", CellValue::Number(66.415452442220769)),
            ((0, 50, 111), "_xlfn.CONFIDENCE.NORM(0.25, 100, 2)", CellValue::Number(81.341984759761857)),
            ((0, 50, 112), "IMPOWER(\"123\", -2.5)", CellValue::Text("5.95987596448829E-06".into())),
            ((0, 50, 113), "IMPOWER(123, -2.5)", CellValue::Text("5.95987596448829E-06".into())),
            ((0, 50, 114), "IMPOWER(\"2024/1/31\", -1)", CellValue::Text("0.0000220643396143153".into())),
            ((0, 50, 115), "IMPOWER(\"2024/1/31\", 2.5)", CellValue::Text("437293141712.532".into())),
            ((0, 50, 116), "SERIESSUM(5, -2.5, 0.5, {2, 1, 0; 1, 3, 1; 0, 1, 4})", CellValue::Number(52.33877742413565)),
            ((0, 50, 117), "NORMSDIST(-2.5)", CellValue::Number(0.006209665325776135)),
            ((0, 50, 118), "ERF(-1, 100)", CellValue::Number(1.842700792949715)),
            ((0, 50, 119), "ERF(0.05, -1)", CellValue::Number(-0.8990727707467315)),
            ((0, 50, 120), "_xlfn.T.INV(0.05, 1)", CellValue::Number(-6.3137515146750438)),
            ((0, 50, 121), "_xlfn.T.INV.2T(0.5, 2)", CellValue::Number(0.816496580927726)),
            ((0, 50, 122), "_xlfn.T.INV.2T(1E-15, 2)", CellValue::Number(31622776.601683773)),
            ((0, 50, 123), "CHIINV(0.25, 1)", CellValue::Number(1.3233036969314653)),
            ((0, 50, 124), "CHIDIST(2.5, 3)", CellValue::Number(0.47529108334302028)),
            ((0, 50, 125), "_xlfn.CHISQ.INV(0.25, 255)", CellValue::Number(239.42646554141962)),
            ((0, 50, 126), "TINV(0, 1)", CellValue::Error(CellError::Div0)),
            ((0, 50, 127), "TINV(0.25, 255)", CellValue::Number(1.1529756638999835)),
            ((0, 50, 128), "NORMDIST(0.25, 100, 10, TRUE)", CellValue::Number(9.8050726443977616e-24)),
            ((0, 50, 129), "LOGNORMDIST(1E-15, 0.25, 3.7)", CellValue::Number(2.6673315017894328e-21)),
            ((0, 50, 130), "_xlfn.BINOM.DIST(2, 255, 0.25, TRUE)", CellValue::Number(5.0930868280557947e-29)),
            ((0, 50, 131), "_xlfn.CHISQ.DIST(3.7, 2, TRUE)", CellValue::Number(0.84276283368637239)),
            ((0, 50, 132), "NEGBINOMDIST(0, 1, 0)", CellValue::Error(CellError::Num)),
            ((0, 50, 133), "NEGBINOMDIST(0, 1, 1)", CellValue::Error(CellError::Num)),
            ((0, 50, 134), "_xlfn.NEGBINOM.DIST(3, 2, 0.05, FALSE)", CellValue::Number(8.5737499999999998e-3)),
            ((0, 50, 135), "POISSON(5, 2.5, FALSE)", CellValue::Number(6.6800942890542642e-2)),
            ((0, 50, 136), "_xlfn.POISSON.DIST(3, 100, TRUE)", CellValue::Number(6.3898877022381386e-39)),
            ((0, 50, 137), "NPV(3.7, 3.7, 3.7, 3.7)", CellValue::Number(0.99036822284079651)),
            ((0, 50, 138), "MIRR(\"abc\", 3.7, 5)", CellValue::Error(CellError::Div0)),
            ((0, 50, 139), "NPER(1E-15, 12, 5, 2, 1)", CellValue::Number(-0.50000000000000044)),
            ((0, 50, 140), "SKEW(5, 3.7, 3.7)", CellValue::Number(1.7320508075688847)),
            ((0, 50, 141), "KURT(3.7, 3.7, 1, 2, 3)", CellValue::Number(-1.0732818569596123)),
            ((0, 50, 142), "_xlfn.SKEW.P(5, 3.7, 3.7)", CellValue::Number(0.70710678118655046)),
            ((0, 50, 143), "PERCENTRANK(C1:C3, 0.25)", CellValue::Error(CellError::NA)),
            ((0, 50, 144), "_xlfn.PERCENTRANK.EXC(C1:C3, 1)", CellValue::Error(CellError::NA)),
            ((0, 50, 145), "_xlfn.PERCENTRANK.INC(A1:A3, 20, 1000)", CellValue::Error(CellError::Div0)),
            ((0, 50, 146), "_xlfn.BINOM.DIST.RANGE(255, 0.5, 2)", CellValue::Number(5.593646373134651e-73)),
            ((0, 50, 147), "_xlfn.CONFIDENCE.T(0.25, 10, 3)", CellValue::Number(9.2582009977255151)),
            ((0, 50, 148), "_xlfn.GAMMA.DIST(0.05, 0.05, 5, FALSE)", CellValue::Number(0.80782854739359444)),
            ((0, 50, 149), "MODE(\"abc\")", CellValue::Error(CellError::NA)),
            ((0, 50, 150), "MODE(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 151), "MODE(\"abc\", 3.7, 3.7)", CellValue::Number(3.7)),
            ((0, 50, 152), "_xlfn.MODE.SNGL(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 153), "_xlfn.CHISQ.TEST(C1:C3, C1:C3)", CellValue::Error(CellError::Div0)),
            ((0, 50, 154), "_xlfn.F.TEST(C1:C3, A1:A3)", CellValue::Error(CellError::Div0)),
            ((0, 50, 155), "_xlfn.T.DIST(0.5, 0, FALSE)", CellValue::Error(CellError::Div0)),
            ((0, 50, 156), "_xlfn.T.DIST(0.5, -1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 157), "_xlfn.T.DIST.RT(-1, 3)", CellValue::Number(0.80449889052211465)),
            ((0, 50, 158), "_xlfn.T.DIST.2T(12, 2)", CellValue::Number(0.006872933677158459)),
            ((0, 50, 159), "CRITBINOM(10, 0, 0.05)", CellValue::Error(CellError::Num)),
            ((0, 50, 160), "CRITBINOM(10, 0.05, 0)", CellValue::Error(CellError::Num)),
            ((0, 50, 161), "CRITBINOM(10, 0.5, 0.05)", CellValue::Number(2.0)),
            ((0, 50, 162), "_xlfn.BINOM.INV(1, 0, 0.5)", CellValue::Error(CellError::Num)),
            ((0, 50, 163), "_xlfn.BINOM.INV(1, 0.5, 0.5)", CellValue::Number(1.0)),
            ((0, 50, 164), "HYPGEOMDIST(0, 1, 3, 3)", CellValue::Number(0.0)),
            ((0, 50, 165), "HYPGEOMDIST(1, 1, 3, 3)", CellValue::Number(1.0)),
            ((0, 50, 166), "HYPGEOMDIST(1, 1, 3, 255)", CellValue::Number(0.011764705882352941)),
            ((0, 50, 167), "_xlfn.HYPGEOM.DIST(0, 2, 0, 10, TRUE)", CellValue::Number(1.0)),
            ((0, 50, 168), "_xlfn.HYPGEOM.DIST(-1, 2, 0, 10, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 50, 169), "WEIBULL(0, 2, 100, FALSE)", CellValue::Number(0.0)),
            ((0, 50, 170), "WEIBULL(3.7, 1E+15, 100, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 171), "_xlfn.WEIBULL.DIST(1000, 1E+15, 1000, TRUE)", CellValue::Number(0.63212055882855767)),
            ((0, 50, 172), "ZTEST(C1:C3, 0)", CellValue::Error(CellError::NA)),
            ((0, 50, 173), "ZTEST(A1, 0)", CellValue::Error(CellError::Div0)),
            ((0, 50, 174), "ZTEST(A1:A3, 0, -1)", CellValue::Error(CellError::Num)),
            ((0, 50, 175), "BETADIST(0.05, 1, 0.05)", CellValue::Number(0.0025613787765302811)),
            ((0, 50, 176), "BETAINV(0, 1, 5)", CellValue::Error(CellError::Num)),
            ((0, 50, 177), "BETAINV(0.25, 1, 123456789)", CellValue::Number(2.3302248053492527e-9)),
            ((0, 50, 178), "_xlfn.BETA.DIST(1000, 100, 1, FALSE, 0.05, 1000)", CellValue::Number(0.0)),
            ((0, 50, 179), "_xlfn.BETA.INV(0, 12, 12)", CellValue::Error(CellError::Num)),
            ((0, 50, 180), "_xlfn.BETA.INV(0.25, 1, 12)", CellValue::Number(0.0236884242226068)),
            ((0, 50, 181), "_xlfn.F.DIST(1, 5, 10, FALSE)", CellValue::Number(0.49547978348663885)),
            ((0, 50, 182), "_xlfn.F.INV(1E-15, 1, 5)", CellValue::Number(1.7348913986289857e-30)),
            ((0, 50, 183), "TTEST(A1:A3, A1:A3, 1, 1)", CellValue::Error(CellError::Div0)),
            ((0, 50, 184), "_xlfn.T.TEST(A1:A3, A1:A3, 1, 1)", CellValue::Error(CellError::Div0)),
            ((0, 50, 185), "_xlfn.LOGNORM.DIST(5, 0.05, 0.05, FALSE)", CellValue::Number(9.4505694153986851e-212)),
            ((0, 50, 186), "_xlfn.LOGNORM.INV(0.05, 5, 10)", CellValue::Number(1.0665073978308865e-5)),
            ((0, 50, 187), "_xlfn.ARRAYTOTEXT(TREND(5), 1)", CellValue::Text("{5}".into())),
            ((0, 50, 188), "_xlfn.ARRAYTOTEXT(GROWTH(5), 1)", CellValue::Text("{5}".into())),
            ((0, 50, 189), "_xlfn.ARRAYTOTEXT(LINEST(5), 1)", CellValue::Text("{0,5}".into())),
            ((0, 50, 190), "_xlfn.ARRAYTOTEXT(LOGEST(5), 1)", CellValue::Text("{1,5}".into())),
            ((0, 50, 191), "TREND(A1:A3, \"abc\")", CellValue::Error(CellError::Ref)),
            ((0, 50, 192), "TREND(\"abc\", A1:A3)", CellValue::Error(CellError::Value)),
            ((0, 50, 193), "CUMIPMT(5, 10, 2, 2, 10, 0)", CellValue::Number(-88.00000165381719)),
            ((0, 50, 194), "CUMIPMT(5, 1000, 2, 2, 10, 0)", CellValue::Number(-90.0)),
            ((0, 50, 195), "CUMIPMT(5, 10, 2, 2, 10, 1)", CellValue::Number(-14.666666942302866)),
            ((0, 50, 196), "CUMPRINC(5, 10, 2, 2, 10, 0)", CellValue::Number(-1.999999834618281)),
            ((0, 50, 197), "CUMIPMT(1E-15, 10, 2, 2, 10, 0)", CellValue::Number(-8.881784197001252e-15)),
            ((0, 50, 198), "PPMT(2.5, 5, 5, 2)", CellValue::Number(-1.4312965722801787)),
            ((0, 50, 199), "PPMT(1E+15, 5, 5, 2)", CellValue::Number(-2.0000000000000027)),
            ((0, 50, 200), "PPMT(123456789, 5, 5, 2)", CellValue::Number(-1.9999999838000018)),
            ((0, 50, 201), "PPMT(1, 5, 5, 1, 0.05, 1)", CellValue::Number(-0.2709677419354839)),
            ((0, 50, 202), "PPMT(0.5, 5, 5, 1, 1, 1)", CellValue::Number(-0.51184834123222744)),
            ((0, 50, 203), "PPMT(0.5, 5, 5, 1, -1, 1)", CellValue::Number(0.0)),
            ((0, 50, 204), "PPMT(1E+15, 5, 5, 1, 0.05, 1)", CellValue::Number(-1.0500000000000025e-15)),
            ((0, 50, 205), "ACCRINT(43831, 43831, 46000, 0.05, 0.05, 4, 0, TRUE)", CellValue::Number(1.4847222222222225e-2)),
            ((0, 50, 206), "ACCRINT(43831, 43831, 46000, 0.05, 0.05, 4, 0, FALSE)", CellValue::Number(1.4847222222222225e-2)),
            ((0, 50, 207), "ACCRINT(43831, 43831, 46000, 0.05, 0.05, 4, 0, \"TRUE\")", CellValue::Error(CellError::Value)),
            ((0, 50, 208), "ACCRINT(0, 44197, 45292, 100, 12, 4)", CellValue::Error(CellError::Num)),
            ((0, 50, 209), "PRICE(43831, 44197, 5, 5, 2, 2, 3)", CellValue::Number(92.0)),
            ((0, 50, 210), "PRICE(43831, 44197, 5, 5, 2, 2, 2)", CellValue::Number(92.0)),
            ((0, 50, 211), "PRICE(0, 44197, 5, 5, 2, 2, 3)", CellValue::Error(CellError::Num)),
            ((0, 50, 212), "PRICEMAT(1, 39551, 39397, 0.061, 0.061)", CellValue::Error(CellError::Num)),
            ((0, 50, 213), "YIELDMAT(39522, 39755, 45000.75, 0.0625, 100.0123)", CellValue::Error(CellError::Num)),
            ((0, 50, 214), "VDB(12, 0.5, 10, 0.25, 5)", CellValue::Number(7.430976)),
            ((0, 50, 215), "VDB(12, 0.5, 10, 0.25, 1)", CellValue::Number(1.7100000000000002)),
            ((0, 50, 216), "VDB(12, 100, 10, 0.25, 5)", CellValue::Number(-88.0)),
            ((0, 50, 217), "VDB(-1, 0.5, 10, 0.25, 5)", CellValue::Error(CellError::Num)),
            ((0, 50, 218), "FISHER(0.5)", CellValue::Number(0.54930614433405489)),
            ((0, 50, 219), "FISHER(1)", CellValue::Error(CellError::Num)),
            ((0, 50, 220), "FISHER(-1)", CellValue::Error(CellError::Num)),
            ((0, 50, 221), "_xlfn.ARRAYTOTEXT(_xlfn.GROUPBY({1;2;1}, {10;20;30}, _xleta.SUM), 1)", CellValue::Text("{1,40;2,20;\"合計\",60}".into())),
            ((0, 50, 222), "_xlfn.ARRAYTOTEXT(_xlfn.GROUPBY({1;2;1}, {10;20;30}, _xleta.SUM, 0, 0, -1), 1)", CellValue::Text("{2,20;1,40}".into())),
            ((0, 50, 223), "_xlfn.ARRAYTOTEXT(_xlfn.PIVOTBY({1;2;1}, {5;5;6}, {10;20;30}, _xleta.SUM), 1)", CellValue::Text("{\"\",5,6,\"合計\";1,10,30,40;2,20,\"\",20;\"合計\",30,30,60}".into())),
            ((0, 50, 224), "_xlfn.ARRAYTOTEXT(_xlfn.BYROW({1,2;3,4}, _xleta.SUM), 1)", CellValue::Text("{3;7}".into())),
            ((0, 50, 226), "_xlfn.ARRAYTOTEXT(_xlfn.GROUPBY({\"大阪\",\"み\";\"大阪\",\"り\";\"東京\",\"り\"}, {1;2;3}, _xleta.SUM, 0, 2), 1)", CellValue::Text("{\"大阪\",\"み\",1;\"大阪\",\"り\",2;\"大阪\",\"\",3;\"東京\",\"り\",3;\"東京\",\"\",3;\"総計\",\"\",6}".into())),
            ((0, 50, 227), "_xlfn.ARRAYTOTEXT(_xlfn.XLOOKUP(_xlfn.SEQUENCE(16, 1, 0), _xlfn.SEQUENCE(8), _xlfn.SEQUENCE(8) * 10, \"none\"), 1)", CellValue::Text("{\"none\";10;20;30;40;50;60;70;80;\"none\";\"none\";\"none\";\"none\";\"none\";\"none\";\"none\"}".into())),
            ((0, 50, 228), "_xlfn.ARRAYTOTEXT(_xlfn.XLOOKUP(_xlfn.SEQUENCE(16, 1, 0) * 0 + 1, _xlfn.SEQUENCE(8), _xlfn.SEQUENCE(8) * 10, 0), 1)", CellValue::Text("{10;10;10;10;10;10;10;10;10;10;10;10;10;10;10;10}".into())),
        ];

        book.sheets.push("Sheet2".to_string());
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 1, 0), CellValue::Number(20.0));
        book.values.insert((0, 2, 0), CellValue::Number(30.0));
        book.values.insert((0, 0, 2), CellValue::Text("apple".to_string()));
        book.values.insert((0, 1, 2), CellValue::Text("".to_string()));
        book.values.insert((0, 2, 2), CellValue::Text("banana".to_string()));
        book.values.insert((0, 0, 5), CellValue::Error(CellError::NA));
        book.values.insert((0, 1, 5), CellValue::Error(CellError::Div0));
        book.values.insert((0, 2, 5), CellValue::Error(CellError::Value));

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_excel_15_digits_and_zero_subtraction() {
        let mut book = MockBook::new(false);
        book.sheets.push("Sheet1".to_string());

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // Comparison operands rounded to 15 digits
            ((0, 0, 0), "0.1+0.2=0.3", CellValue::Bool(true)),
            ((0, 0, 1), "1-0.9=0.1", CellValue::Bool(true)),
            // Root subtraction/addition near-zero compensation
            ((0, 0, 2), "0.1+0.2-0.3", CellValue::Number(0.0)),
            ((0, 0, 3), "1-0.9-0.1", CellValue::Number(0.0)),
            ((0, 0, 4), "(0.1+0.2)-0.3", CellValue::Number(0.0)),
            ((0, 0, 5), "0.3-(0.1+0.2)", CellValue::Number(0.0)),
            // Non-root subtraction preserved as IEEE-754: 5.55e-17 != 0
            ((0, 0, 6), "(0.1+0.2)-0.3=0", CellValue::Bool(false)),
            // Lexer 15-digit precision capping for literals
            ((0, 0, 7), "12345678901234567", CellValue::Number(12345678901234500.0)),
            ((0, 0, 8), "123456789012345678+1", CellValue::Number(123456789012345000.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }
}


