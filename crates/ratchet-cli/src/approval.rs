use dialoguer::{Select, theme::ColorfulTheme};
use ratchet_sandbox::{ApprovalDecision, ApprovalHandler};
use std::io::IsTerminal;

/// Interactive approval prompt backed by a terminal selection menu.
///
/// Falls back to a safe denial when there is no TTY (CI, ACP-driven runs) so
/// an unattended process can never silently execute a risky command.
pub struct CliApprovalHandler;

impl ApprovalHandler for CliApprovalHandler {
    fn request(&self, action: &str, details: &str) -> ApprovalDecision {
        if !std::io::stdin().is_terminal() {
            eprintln!(
                "⛔ '{action}' needs approval but no terminal is attached; denying.\n   {details}"
            );
            return ApprovalDecision::Reject;
        }

        println!("\n⚠️  Approval required: {action}");
        for line in details.lines() {
            println!("   {line}");
        }

        let options = [
            "Allow once",
            "Allow for this session",
            "Deny",
            "Deny for this session",
        ];

        match Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Proceed?")
            .items(&options)
            .default(2)
            .interact()
        {
            Ok(0) => ApprovalDecision::Approve,
            Ok(1) => ApprovalDecision::ApproveAlways,
            Ok(2) => ApprovalDecision::Reject,
            Ok(3) => ApprovalDecision::RejectAlways,
            _ => ApprovalDecision::Reject,
        }
    }
}
