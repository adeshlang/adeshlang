//! Debug information handling and stripping.

use crate::object::ObjectFile;

pub struct DebugProcessor;

impl DebugProcessor {
    pub fn strip_debug_sections(objects: &mut [ObjectFile], strip_all: bool, strip_debug: bool) {
        if !strip_all && !strip_debug {
            return;
        }

        for obj in objects.iter_mut() {
            obj.sections.retain(|sec| {
                if strip_all {
                    !sec.name.starts_with(".debug") && !sec.name.starts_with(".comment")
                } else if strip_debug {
                    !sec.name.starts_with(".debug")
                } else {
                    true
                }
            });
        }
    }
}
