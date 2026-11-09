//! Who may do what. This table is the only place that decides it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    Admin,
    Lead,
    Developer,
    Contributor,
    Viewer,
}

impl TeamRole {
    pub const ALL: [Self; 5] = [
        Self::Admin,
        Self::Lead,
        Self::Developer,
        Self::Contributor,
        Self::Viewer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Lead => "lead",
            Self::Developer => "developer",
            Self::Contributor => "contributor",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.name() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// See the project, its versions, comments and locks.
    Read,
    /// Rename or delete the team.
    ManageTeam,
    /// Invite, remove and change members, within what `may_assign` allows.
    ManageMembers,
    /// Add projects to the team.
    CreateProject,
    /// Assign areas and project roles, end someone else's lock.
    ManageProject,
    /// Read the audit log.
    ViewAudit,
    /// Write comments.
    Comment,
    /// Change any area and lock it, if nobody else holds it.
    WriteAnyArea,
    /// Change and lock an area assigned to this person.
    WriteAssignedArea,
}

impl Action {
    pub const ALL: [Self; 9] = [
        Self::Read,
        Self::ManageTeam,
        Self::ManageMembers,
        Self::CreateProject,
        Self::ManageProject,
        Self::ViewAudit,
        Self::Comment,
        Self::WriteAnyArea,
        Self::WriteAssignedArea,
    ];
}

pub fn allows(role: TeamRole, action: Action) -> bool {
    use Action as A;
    use TeamRole as R;
    match action {
        A::Read => true,
        A::ManageTeam => role == R::Admin,
        A::ManageMembers | A::CreateProject | A::ManageProject | A::ViewAudit => {
            matches!(role, R::Admin | R::Lead)
        }
        A::WriteAnyArea => matches!(role, R::Admin | R::Lead | R::Developer),
        A::Comment | A::WriteAssignedArea => {
            matches!(role, R::Admin | R::Lead | R::Developer | R::Contributor)
        }
    }
}

/// A project lead manages the people below, but cannot make or touch leads and
/// administrators, so that nobody can raise their own rights.
pub fn may_assign(actor: TeamRole, target: TeamRole) -> bool {
    match actor {
        TeamRole::Admin => true,
        TeamRole::Lead => matches!(
            target,
            TeamRole::Developer | TeamRole::Contributor | TeamRole::Viewer
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole table, written out once more, so that a change to it cannot go
    /// unnoticed.
    #[test]
    fn the_table_is_what_was_agreed() {
        let expected: [(Action, [bool; 5]); 9] = [
            //                        adm    lead   dev    contr  view
            (Action::Read, [true, true, true, true, true]),
            (Action::ManageTeam, [true, false, false, false, false]),
            (Action::ManageMembers, [true, true, false, false, false]),
            (Action::CreateProject, [true, true, false, false, false]),
            (Action::ManageProject, [true, true, false, false, false]),
            (Action::ViewAudit, [true, true, false, false, false]),
            (Action::Comment, [true, true, true, true, false]),
            (Action::WriteAnyArea, [true, true, true, false, false]),
            (Action::WriteAssignedArea, [true, true, true, true, false]),
        ];
        assert_eq!(expected.len(), Action::ALL.len());
        for (action, row) in expected {
            for (role, allowed) in TeamRole::ALL.into_iter().zip(row) {
                assert_eq!(allows(role, action), allowed, "{role:?} {action:?}");
            }
        }
    }

    #[test]
    fn nobody_raises_their_own_rights() {
        for target in TeamRole::ALL {
            assert!(may_assign(TeamRole::Admin, target));
            assert!(!may_assign(TeamRole::Developer, target));
            assert!(!may_assign(TeamRole::Contributor, target));
            assert!(!may_assign(TeamRole::Viewer, target));
        }
        assert!(!may_assign(TeamRole::Lead, TeamRole::Admin));
        assert!(!may_assign(TeamRole::Lead, TeamRole::Lead));
        assert!(may_assign(TeamRole::Lead, TeamRole::Viewer));
    }

    #[test]
    fn names_read_back() {
        for role in TeamRole::ALL {
            assert_eq!(TeamRole::parse(role.name()), Some(role));
        }
        assert_eq!(TeamRole::parse("owner"), None);
    }
}
