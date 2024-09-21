use skylite_proc::skylite_project;

#[skylite_project("./tests/test-project-1/project.scm", skylite_mock::MockTarget)]
mod project1 {

    #[skylite_proc::pre_update]
    fn pre_update(project: &mut TestProject1) {

    }
}
