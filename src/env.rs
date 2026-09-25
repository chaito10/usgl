use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::{RtResult, UsglError};
use crate::value::{FuncKind, Value};

/// A scope chain. Modules are also represented by an `Env` populated with
/// their bindings (that is how `fs.list`, `database.query`, ... resolve).
pub struct Env {
    vars: RefCell<HashMap<String, Slot>>,
    pub parent: Option<Rc<Env>>,
    pub name: Option<String>,
    /// When true, redefining an existing name in this scope is allowed
    /// (used by the REPL).
    allow_redefine: Cell<bool>,
}

pub struct Slot {
    pub value: Value,
    pub mutable: bool,
    /// Runtime approximation of ownership moves: a value that was consumed
    /// by passing it by value is marked moved; using it again is an error.
    pub moved: bool,
}

impl Env {
    pub fn new(parent: Option<Rc<Env>>, name: Option<String>) -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(HashMap::new()),
            parent,
            name,
            allow_redefine: Cell::new(false),
        })
    }

    pub fn allow_redefine(&self) {
        self.allow_redefine.set(true);
    }

    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("<global>")
    }

    pub fn define(&self, name: &str, value: Value, mutable: bool) -> RtResult<()> {
        let mut vars = self.vars.borrow_mut();
        let is_builtin = vars
            .get(name)
            .map(|s| match &s.value {
                Value::Function(f) => matches!(f.kind, FuncKind::Builtin(_)),
                Value::Module(_) => true,
                _ => false,
            })
            .unwrap_or(false);
        if vars.contains_key(name) && !self.allow_redefine.get() && !is_builtin {
            return Err(UsglError::rt(
                format!("`{}` is already defined in this scope", name),
                None,
            ));
        }
        vars.insert(
            name.to_string(),
            Slot {
                value,
                mutable,
                moved: false,
            },
        );
        Ok(())
    }

    pub fn define_anyway(&self, name: &str, value: Value, mutable: bool) {
        let mut vars = self.vars.borrow_mut();
        vars.insert(
            name.to_string(),
            Slot {
                value,
                mutable,
                moved: false,
            },
        );
    }

    pub fn has_current(&self, name: &str) -> bool {
        self.vars.borrow().contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let mut e: &Env = self;
        loop {
            if let Some(slot) = e.vars.borrow().get(name) {
                return if slot.moved {
                    None
                } else {
                    Some(slot.value.clone())
                };
            }
            match &e.parent {
                Some(p) => e = p,
                None => return None,
            }
        }
    }

    pub fn assign(&self, name: &str, value: Value) -> RtResult<()> {
        let mut e: &Env = self;
        loop {
            let var_slot = {
                let mut vars = e.vars.borrow_mut();
                vars.get_mut(name).map(|slot| {
                    if !slot.mutable {
                        return Err(UsglError::rt(
                            format!("cannot assign to immutable binding `{}` (use `var`)", name),
                            None,
                        ));
                    }
                    if slot.moved {
                        return Err(UsglError::rt(
                            format!("cannot assign to `{}`: value was moved", name),
                            None,
                        ));
                    }
                    Ok(())
                })
            };
            if let Some(res) = var_slot {
                if res.is_ok() {
                    let mut vars = e.vars.borrow_mut();
                    vars.get_mut(name).unwrap().value = value;
                }
                return res;
            }
            match &e.parent {
                Some(p) => e = p,
                None => return Err(UsglError::rt(format!("unknown variable `{}`", name), None)),
            }
        }
    }

    pub fn assign_current(&self, name: &str, value: Value) -> RtResult<()> {
        let mut vars = self.vars.borrow_mut();
        if let Some(slot) = vars.get_mut(name) {
            if !slot.mutable {
                return Err(UsglError::rt(
                    format!("cannot assign to immutable binding `{}` (use `var`)", name),
                    None,
                ));
            }
            slot.value = value;
            Ok(())
        } else {
            Err(UsglError::rt(format!("unknown variable `{}`", name), None))
        }
    }

    pub fn bindings(&self) -> Vec<(String, Value)> {
        self.vars
            .borrow()
            .iter()
            .map(|(k, s)| (k.clone(), s.value.clone()))
            .collect()
    }
}
