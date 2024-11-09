mod actors;

use skylite_proc::skylite_project;

#[skylite_project("./tests/test-project-1/project.scm", skylite_mock::MockTarget)]
pub mod project1 {
    use crate::actors::*;

    #[skylite_proc::pre_update]
    fn pre_update(project: &mut TestProject1) {

    }
}
