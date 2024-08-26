use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::AtomicU64;
use crate::privacy::Loggable;

static TASK_ID: AtomicU64 = AtomicU64::new(0);



static CONTEXT_ID: AtomicU64 = AtomicU64::new(0);



#[derive(Copy,Clone,Debug,PartialEq,Eq,Hash)]
pub struct TaskID(u64);
impl Display for TaskID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Copy,Clone,Debug,PartialEq,Eq,Hash)]
pub struct ContextID(u64);
impl Display for ContextID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}



/**
Provides a set of info that can be used by multiple logs.
*/
pub struct Context {
    parent: Option<Box<Context>>,
    values: HashMap<&'static str, Box<dyn Loggable>>,
    task_id: u64,
    context_id: u64
}

thread_local! {
    static CONTEXT: Cell<Option<Context>> = const{Cell::new(None)};
}

impl Context {
    /**
    Determines whether a context is currently set.
    */
    #[inline]
    pub fn has_current() -> bool {
        CONTEXT.with(|c| {
            let context = c.take();
            let result = context.is_some();
            c.set(context);
            result
        })
    }
    /**
    Returns a raw pointer to the thread-local context.

    When using this pointer, you must ensure that the context is only accessed from the local thread,
    and is not mutated by the local thread while the pointer is in use.
    */
    #[inline]
    pub fn current() -> *const Option<Context> {
        CONTEXT.with(|c| {
            c.as_ptr()
        })
    }
    /**
    Creates a new orphaned context
    */
    #[inline]
    pub fn new_orphan() -> Context {
        Context {
            parent: None,
            values: HashMap::new(),
            task_id: TASK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            context_id: CONTEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }
    }

    /**
    Sets a blank context
    */
    #[inline]
    pub fn reset() {
        CONTEXT.with(|c| c.set(Some(Context::new_orphan())));
    }

    #[inline]
    pub fn task_id(&self) -> TaskID {
        TaskID(self.task_id)
    }

    /**
    Sets the current context to this one.
    */
    pub fn set_current(self) {
        CONTEXT.set(Some(self));
    }

    /**
    Finds the number of nesting contexts.
    */
    pub fn nesting_level(&self) -> usize {
        let mut level = 0;
        let mut current = self;
        while let Some(parent) = &current.parent {
            level += 1;
            current = parent;
        }
        level
    }

    /**
    Returns the ID of the current context.
    */
    #[inline]
    pub fn context_id(&self) -> ContextID {
        ContextID(self.context_id)
    }

    /**
    Pushes the current context onto the stack and sets this context as the current one.
    */
    pub(crate) fn new_push() -> ContextID {
        let current = CONTEXT.with(|c| c.take()).unwrap();
        let task_id = current.task_id().0;
        let new_context = Context {
            parent: Some(Box::new(current)),
            values: HashMap::new(),
            task_id,
            context_id: CONTEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        };
        let id = new_context.context_id();
        new_context.set_current();
        id
    }

    /**
    Pops the context with specified ID.

    If the context cannot be found, panics.
    */
    pub(crate)fn pop(id: ContextID) {
        let mut current = CONTEXT.with(|c| c.take()).unwrap();
        loop {
            if current.context_id() == id {
                let parent = current.parent.take().unwrap();
                parent.set_current();
                return;
            }
            let parent = current.parent.take().unwrap();
            current = *parent;
        }
    }


    pub fn set<L: Loggable + 'static>(&mut self, key: &'static str, value: L) {
        self.values.insert(key, Box::new(value));
    }

    /**
    Internal implementation detail of the logging system.
    */
    #[inline]
    pub unsafe fn _log_current_context(file: &'static str,line: u32, column: u32) -> &'static Context {
        use crate::logger::Logger;
        match (&*Context::current()).as_ref() {
            None => {
                /*
                Create an additional warning about the missing context.
                 */
                let mut record = crate::log_record::LogRecord::new();
                record.log("WARN: ");

                //file, line
                record.log(file);
                record.log_owned(format!(":{}:{} ",line,column));

                //for warn, we can afford timestamp
                record.log_timestamp();
                record.log("No context found. Creating orphan context.");

                let global_logger = &crate::global_logger::GLOBAL_LOGGER;
                global_logger.finish_log_record(record);
                let new_ctx = Context::new_orphan();
                new_ctx.set_current();
                (&*Context::current()).as_ref().unwrap()
            }
            Some(ctx) => {
                ctx
            }

        }
    }
    /**
    Internal implementation detail of the logging system.

    Logs the start of logs that typically use Context.
    */
    #[inline] pub fn _log_prelude(&self, record: &mut crate::log_record::LogRecord) {
        for _ in 0..self.nesting_level() {
            record.log(" ");
        }
        record.log_owned(format!("{} ",self.task_id()));
    }
}