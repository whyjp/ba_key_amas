//! Asking for the member code from the add-on's card, which has no terminal to type into.
//!
//! A small dialog with a hidden-input box is shown (a Windows Forms form run by PowerShell, which every Windows has):
//! the code never reaches a command line or a log. Only its hash is kept afterwards (`upload::resolve_account_id`).
use anyhow::{anyhow, bail, Result};
use std::process::Command;

/// The dialog's script; `prompt` is what `upload::resolve_account_id` asks ("회원코드: ", "한 번 더: ").
fn script_for(prompt: &str) -> String {
    let what = prompt.trim().trim_end_matches(':').replace('\'', "");
    format!(
        "Add-Type -AssemblyName System.Windows.Forms;          $f = New-Object Windows.Forms.Form; $f.Text = 'ba-key-AMAS'; $f.Width = 380; $f.Height = 170; $f.StartPosition = 'CenterScreen'; $f.TopMost = $true; $f.FormBorderStyle = 'FixedDialog'; $f.MaximizeBox = $false;          $l = New-Object Windows.Forms.Label; $l.Text = '{what}: 회원코드를 입력하세요 (화면에 표시되지 않습니다)'; $l.Left = 12; $l.Top = 12; $l.Width = 340;          $t = New-Object Windows.Forms.TextBox; $t.Left = 12; $t.Top = 40; $t.Width = 340; $t.UseSystemPasswordChar = $true;          $o = New-Object Windows.Forms.Button; $o.Text = '확인'; $o.Left = 180; $o.Top = 80; $o.DialogResult = 'OK';          $c = New-Object Windows.Forms.Button; $c.Text = '취소'; $c.Left = 270; $c.Top = 80; $c.DialogResult = 'Cancel';          $f.AcceptButton = $o; $f.CancelButton = $c; $f.Controls.AddRange(@($l, $t, $o, $c));          if ($f.ShowDialog() -eq 'OK') {{ $t.Text }}"
    )
}

/// A hidden-input prompt answered in a dialog; an error when it is cancelled or cannot be shown.
pub fn ask(prompt: &str) -> Result<String> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-STA", "-Command", &script_for(prompt)]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: the dialog is shown, the console window is not
    }
    let out = cmd.output().map_err(|e| anyhow!("회원코드 입력 창을 열지 못했습니다: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        bail!("회원코드 입력을 취소했습니다");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_text_names_what_is_asked_and_cannot_break_out_of_its_quotes() {
        let s = script_for("한 번 더: ");
        assert!(s.contains("한 번 더: 회원코드를"), "{s}");
        assert!(!script_for("a'b; Remove-Item x").contains("a'b"), "a quote in the prompt is dropped");
    }
}
