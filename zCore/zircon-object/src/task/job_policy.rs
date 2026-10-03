use crate::error::*;
use crate::signal::Slack;

/// V2 override flags for policy conditions.
pub const POL_OVERRIDE_ALLOW: u32 = 0;
pub const POL_OVERRIDE_DENY: u32 = 1;

/// Security and resource policies of a job.
#[derive(Default, Copy, Clone)]
pub struct JobPolicy {
    // TODO: use bitset
    action: [Option<PolicyAction>; 15],
    /// Per-condition override flag. `true` means override is allowed (V2).
    /// For V1 policies (no flags field), defaults to `false` (deny override).
    override_allow: [bool; 15],
}

impl JobPolicy {
    /// Get the action of a policy `condition`.
    pub fn get_action(&self, condition: PolicyCondition) -> Option<PolicyAction> {
        self.action[condition as usize]
    }

    /// Check if the override flag allows changing this condition.
    pub fn is_override_allowed(&self, condition: PolicyCondition) -> bool {
        self.override_allow[condition as usize]
    }

    /// Apply a basic V1 policy (override defaults to deny).
    pub fn apply(&mut self, policy: BasicPolicy) {
        self.action[policy.condition as usize] = Some(policy.action);
    }

    /// Apply a basic V2 policy with override flag.
    pub fn apply_v2(&mut self, policy: BasicPolicyV2) {
        let idx = policy.condition as usize;
        self.action[idx] = Some(policy.action);
        self.override_allow[idx] = policy.flags == POL_OVERRIDE_ALLOW;
    }

    /// Merge the policy with `parent`'s.
    pub fn merge(&self, parent: &Self) -> Self {
        let mut new = *self;
        for i in 0..15 {
            if parent.action[i].is_some() {
                new.action[i] = parent.action[i];
            }
        }
        new
    }
}

/// Control the effect in the case of conflict between
/// the existing policies and the new policies when setting new policies.
#[derive(Debug, Copy, Clone)]
pub enum SetPolicyOptions {
    /// Policy is applied for all conditions in policy or the call fails.
    Absolute,
    /// Policy is applied for the conditions not specifically overridden by the parent policy.
    Relative,
}

/// The policy type (V1 format, 8 bytes) as read from userspace.
/// Uses raw u32 fields to avoid UB from invalid enum discriminants.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct BasicPolicyRaw {
    pub condition: u32,
    pub action: u32,
}

impl BasicPolicyRaw {
    /// Validate and convert raw ABI fields to typed policy.
    pub fn validate(&self) -> ZxResult<BasicPolicy> {
        let condition = PolicyCondition::try_from(self.condition)?;
        let action = PolicyAction::try_from(self.action)?;
        Ok(BasicPolicy { condition, action })
    }
}

/// Validated V1 policy entry.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct BasicPolicy {
    /// Condition when the policy is applied.
    pub condition: PolicyCondition,
    /// Action to take when the policy is applied.
    pub action: PolicyAction,
}

/// The policy type (V2 format, 12 bytes) as read from userspace.
/// Uses raw u32 fields to avoid UB from invalid enum discriminants.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct BasicPolicyV2Raw {
    pub condition: u32,
    pub action: u32,
    pub flags: u32,
}

/// Validated V2 policy entry.
#[derive(Debug, Copy, Clone)]
pub struct BasicPolicyV2 {
    /// Condition when the policy is applied.
    pub condition: PolicyCondition,
    /// Action to take when the policy is applied.
    pub action: PolicyAction,
    /// Override flags: 0 = OVERRIDE_ALLOW, 1 = OVERRIDE_DENY.
    pub flags: u32,
}

impl BasicPolicyV2Raw {
    /// Validate and convert raw ABI fields to typed policy.
    pub fn validate(&self) -> ZxResult<BasicPolicyV2> {
        Ok(BasicPolicyV2 {
            condition: PolicyCondition::try_from(self.condition)?,
            action: PolicyAction::try_from(self.action)?,
            flags: self.flags,
        })
    }
}

/// The condition when a policy is applied.
#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum PolicyCondition {
    /// A process under this job is attempting to issue a syscall with an invalid handle.
    /// In this case, `PolicyAction::Allow` and `PolicyAction::Deny` are equivalent:
    /// if the syscall returns, it will always return the error ZX_ERR_BAD_HANDLE.
    BadHandle = 0,
    /// A process under this job is attempting to issue a syscall with a handle that does not support such operation.
    WrongObject = 1,
    /// A process under this job is attempting to map an address region with write-execute access.
    VmarWx = 2,
    /// A special condition that stands for all of the above ZX_NEW conditions
    /// such as NEW_VMO, NEW_CHANNEL, NEW_EVENT, NEW_EVENTPAIR, NEW_PORT, NEW_SOCKET, NEW_FIFO,
    /// And any future ZX_NEW policy.
    /// This will include any new kernel objects which do not require a parent object for creation.
    NewAny = 3,
    /// A process under this job is attempting to create a new vm object.
    NewVMO = 4,
    /// A process under this job is attempting to create a new channel.
    NewChannel = 5,
    /// A process under this job is attempting to create a new event.
    NewEvent = 6,
    /// A process under this job is attempting to create a new event pair.
    NewEventPair = 7,
    /// A process under this job is attempting to create a new port.
    NewPort = 8,
    /// A process under this job is attempting to create a new socket.
    NewSocket = 9,
    /// A process under this job is attempting to create a new fifo.
    NewFIFO = 10,
    /// A process under this job is attempting to create a new timer.
    NewTimer = 11,
    /// A process under this job is attempting to create a new process.
    NewProcess = 12,
    /// A process under this job is attempting to create a new profile.
    NewProfile = 13,
    /// A process under this job is attempting to use zx_vmo_replace_as_executable()
    /// with a ZX_HANDLE_INVALID as the second argument rather than a valid ZX_RSRC_KIND_VMEX.
    AmbientMarkVMOExec = 14,
}

/// The action taken when the condition happens specified by a policy.
#[repr(u32)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PolicyAction {
    /// Allow condition.
    Allow = 0,
    /// Prevent condition.
    Deny = 1,
    /// Generate an exception via the debug port. An exception generated this
    /// way acts as a breakpoint. The thread may be resumed after the exception.
    AllowException = 2,
    /// Just like `AllowException`, but after resuming condition is denied.
    DenyException = 3,
    /// Terminate the process.
    Kill = 4,
}

impl core::convert::TryFrom<u32> for PolicyCondition {
    type Error = ZxError;
    fn try_from(v: u32) -> ZxResult<Self> {
        match v {
            0 => Ok(Self::BadHandle),
            1 => Ok(Self::WrongObject),
            2 => Ok(Self::VmarWx),
            3 => Ok(Self::NewAny),
            4 => Ok(Self::NewVMO),
            5 => Ok(Self::NewChannel),
            6 => Ok(Self::NewEvent),
            7 => Ok(Self::NewEventPair),
            8 => Ok(Self::NewPort),
            9 => Ok(Self::NewSocket),
            10 => Ok(Self::NewFIFO),
            11 => Ok(Self::NewTimer),
            12 => Ok(Self::NewProcess),
            13 => Ok(Self::NewProfile),
            14 => Ok(Self::AmbientMarkVMOExec),
            _ => Err(ZxError::INVALID_ARGS),
        }
    }
}

impl core::convert::TryFrom<u32> for PolicyAction {
    type Error = ZxError;
    fn try_from(v: u32) -> ZxResult<Self> {
        match v {
            0 => Ok(Self::Allow),
            1 => Ok(Self::Deny),
            2 => Ok(Self::AllowException),
            3 => Ok(Self::DenyException),
            4 => Ok(Self::Kill),
            _ => Err(ZxError::INVALID_ARGS),
        }
    }
}

/// Timer slack policy.
///
/// See [timer slack](../../signal/timer/enum.Slack.html) for more information.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TimerSlackPolicy {
    min_slack: i64,
    /// Raw u32 from userspace — must be validated as a valid Slack variant.
    default_mode: u32,
}

impl TimerSlackPolicy {
    /// Get the default_mode as a validated Slack enum.
    pub fn slack_mode(&self) -> ZxResult<Slack> {
        match self.default_mode {
            0 => Ok(Slack::Center),
            1 => Ok(Slack::Early),
            2 => Ok(Slack::Late),
            _ => Err(ZxError::INVALID_ARGS),
        }
    }
}

/// Check whether the policy is valid.
pub fn check_timer_policy(policy: &TimerSlackPolicy) -> ZxResult {
    if policy.min_slack.is_negative() {
        return Err(ZxError::INVALID_ARGS);
    }
    // Validate that default_mode is a known Slack variant (0=Center, 1=Early, 2=Late).
    let _ = policy.slack_mode()?;
    Ok(())
}

#[repr(C)]
pub(super) struct TimerSlack {
    amount: i64,
    mode: Slack,
}

impl TimerSlack {
    pub(super) fn generate_new(&self, policy: TimerSlackPolicy) -> TimerSlack {
        // slack_mode() was already validated by check_timer_policy, so unwrap is safe.
        TimerSlack {
            amount: self.amount.max(policy.min_slack),
            mode: policy.slack_mode().unwrap_or(Slack::Center),
        }
    }
}

impl Default for TimerSlack {
    fn default() -> Self {
        TimerSlack {
            amount: 0,
            mode: Slack::Center,
        }
    }
}
