use serde_json::Value;

use super::cdp::client::CdpClient;
use super::cdp::types::*;
use super::element::{resolve_element_center, resolve_element_object_id, RefMap};

pub async fn click(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
    button: &str,
    click_count: i32,
) -> Result<(), String> {
    let (x, y) = resolve_element_center(client, session_id, ref_map, selector_or_ref).await?;
    dispatch_click(client, session_id, x, y, button, click_count).await
}

pub async fn dblclick(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    click(client, session_id, ref_map, selector_or_ref, "left", 2).await
}

pub async fn hover(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let (x, y) = resolve_element_center(client, session_id, ref_map, selector_or_ref).await?;
    client
        .send_command_typed::<_, Value>(
            "Input.dispatchMouseEvent",
            &DispatchMouseEventParams {
                event_type: "mouseMoved".to_string(),
                x,
                y,
                button: None,
                buttons: None,
                click_count: None,
                delta_x: None,
                delta_y: None,
                modifiers: None,
            },
            Some(session_id),
        )
        .await?;
    Ok(())
}

pub async fn fill(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
    value: &str,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    // Focus the element
    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: "function() { this.focus(); }".to_string(),
                object_id: Some(object_id.clone()),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    // Select all + delete to clear
    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: r#"function() {
                    this.select && this.select();
                    this.value = '';
                    this.dispatchEvent(new Event('input', { bubbles: true }));
                }"#
                .to_string(),
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    // Insert text
    client
        .send_command_typed::<_, Value>(
            "Input.insertText",
            &InsertTextParams {
                text: value.to_string(),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn type_text(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
    text: &str,
    clear: bool,
    delay_ms: Option<u64>,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    // Focus
    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: "function() { this.focus(); }".to_string(),
                object_id: Some(object_id.clone()),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    if clear {
        client
            .send_command_typed::<_, Value>(
                "Runtime.callFunctionOn",
                &CallFunctionOnParams {
                    function_declaration: r#"function() {
                        this.select && this.select();
                        this.value = '';
                        this.dispatchEvent(new Event('input', { bubbles: true }));
                    }"#
                    .to_string(),
                    object_id: Some(object_id),
                    arguments: None,
                    return_by_value: Some(true),
                    await_promise: Some(false),
                },
                Some(session_id),
            )
            .await?;
    }

    let delay = delay_ms.unwrap_or(0);

    for ch in text.chars() {
        let text_str = ch.to_string();
        let (key, code, key_code) = char_to_key_info(ch);

        client
            .send_command_typed::<_, Value>(
                "Input.dispatchKeyEvent",
                &DispatchKeyEventParams {
                    event_type: "keyDown".to_string(),
                    key: Some(key.clone()),
                    code: Some(code.clone()),
                    text: Some(text_str.clone()),
                    unmodified_text: Some(text_str.clone()),
                    windows_virtual_key_code: Some(key_code),
                    native_virtual_key_code: Some(key_code),
                    modifiers: None,
                },
                Some(session_id),
            )
            .await?;

        client
            .send_command_typed::<_, Value>(
                "Input.dispatchKeyEvent",
                &DispatchKeyEventParams {
                    event_type: "keyUp".to_string(),
                    key: Some(key),
                    code: Some(code),
                    text: None,
                    unmodified_text: None,
                    windows_virtual_key_code: Some(key_code),
                    native_virtual_key_code: Some(key_code),
                    modifiers: None,
                },
                Some(session_id),
            )
            .await?;

        if delay > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }
    }

    Ok(())
}

pub async fn press_key(client: &CdpClient, session_id: &str, key: &str) -> Result<(), String> {
    press_key_with_modifiers(client, session_id, key, None).await
}

/// Dispatch a keyDown+keyUp sequence for `key` with an optional CDP modifier bitmask.
///
/// Modifier values follow the CDP `Input.dispatchKeyEvent` spec:
/// 1 = Alt, 2 = Control, 4 = Meta (Cmd), 8 = Shift.
///
/// Callers that need a platform-appropriate modifier (e.g. Cmd on macOS,
/// Ctrl elsewhere) must choose the value themselves -- see `cfg!(target_os)`.
pub async fn press_key_with_modifiers(
    client: &CdpClient,
    session_id: &str,
    key: &str,
    modifiers: Option<i32>,
) -> Result<(), String> {
    let (key_name, code, key_code) = named_key_info(key);

    client
        .send_command_typed::<_, Value>(
            "Input.dispatchKeyEvent",
            &DispatchKeyEventParams {
                event_type: "keyDown".to_string(),
                key: Some(key_name.clone()),
                code: Some(code.clone()),
                text: None,
                unmodified_text: None,
                windows_virtual_key_code: Some(key_code),
                native_virtual_key_code: Some(key_code),
                modifiers,
            },
            Some(session_id),
        )
        .await?;

    client
        .send_command_typed::<_, Value>(
            "Input.dispatchKeyEvent",
            &DispatchKeyEventParams {
                event_type: "keyUp".to_string(),
                key: Some(key_name),
                code: Some(code),
                text: None,
                unmodified_text: None,
                windows_virtual_key_code: Some(key_code),
                native_virtual_key_code: Some(key_code),
                modifiers,
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn scroll(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: Option<&str>,
    delta_x: f64,
    delta_y: f64,
) -> Result<(), String> {
    if let Some(sel) = selector_or_ref {
        let object_id = resolve_element_object_id(client, session_id, ref_map, sel).await?;
        let js = "function(dx, dy) { this.scrollBy(dx, dy); }".to_string();
        client
            .send_command_typed::<_, Value>(
                "Runtime.callFunctionOn",
                &CallFunctionOnParams {
                    function_declaration: js,
                    object_id: Some(object_id),
                    arguments: Some(vec![
                        CallArgument {
                            value: Some(serde_json::json!(delta_x)),
                            object_id: None,
                        },
                        CallArgument {
                            value: Some(serde_json::json!(delta_y)),
                            object_id: None,
                        },
                    ]),
                    return_by_value: Some(true),
                    await_promise: Some(false),
                },
                Some(session_id),
            )
            .await?;
    } else {
        let js = format!("window.scrollBy({}, {})", delta_x, delta_y);
        client
            .send_command_typed::<_, Value>(
                "Runtime.evaluate",
                &EvaluateParams {
                    expression: js,
                    return_by_value: Some(true),
                    await_promise: Some(false),
                },
                Some(session_id),
            )
            .await?;
    }
    Ok(())
}

pub async fn select_option(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
    values: &[String],
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    let js = r#"function(vals) {
            const options = Array.from(this.options);
            for (const opt of options) {
                opt.selected = vals.includes(opt.value) || vals.includes(opt.textContent.trim());
            }
            this.dispatchEvent(new Event('change', { bubbles: true }));
        }"#
    .to_string();

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: js,
                object_id: Some(object_id),
                arguments: Some(vec![CallArgument {
                    value: Some(serde_json::json!(values)),
                    object_id: None,
                }]),
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn check(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let is_checked =
        super::element::is_element_checked(client, session_id, ref_map, selector_or_ref).await?;
    if !is_checked {
        click(client, session_id, ref_map, selector_or_ref, "left", 1).await?;
    }
    Ok(())
}

pub async fn uncheck(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let is_checked =
        super::element::is_element_checked(client, session_id, ref_map, selector_or_ref).await?;
    if is_checked {
        click(client, session_id, ref_map, selector_or_ref, "left", 1).await?;
    }
    Ok(())
}

pub async fn focus(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: "function() { this.focus(); }".to_string(),
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn clear(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: r#"function() {
                    this.focus();
                    this.value = '';
                    this.dispatchEvent(new Event('input', { bubbles: true }));
                    this.dispatchEvent(new Event('change', { bubbles: true }));
                }"#
                .to_string(),
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn select_all(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: r#"function() {
                    this.focus();
                    if (typeof this.select === 'function') {
                        this.select();
                    } else {
                        const range = document.createRange();
                        range.selectNodeContents(this);
                        const sel = window.getSelection();
                        sel.removeAllRanges();
                        sel.addRange(range);
                    }
                }"#
                .to_string(),
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn scroll_into_view(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration:
                    "function() { this.scrollIntoView({ block: 'center', inline: 'center' }); }"
                        .to_string(),
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn dispatch_event(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
    event_type: &str,
    event_init: Option<&Value>,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    let init_json = event_init
        .map(|v| serde_json::to_string(v).unwrap_or("{}".to_string()))
        .unwrap_or_else(|| "{ bubbles: true }".to_string());

    let js = format!(
        "function() {{ this.dispatchEvent(new Event({}, {})); }}",
        serde_json::to_string(event_type).unwrap_or_default(),
        init_json
    );

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: js,
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn highlight(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let object_id = resolve_element_object_id(client, session_id, ref_map, selector_or_ref).await?;

    client
        .send_command_typed::<_, Value>(
            "Runtime.callFunctionOn",
            &CallFunctionOnParams {
                function_declaration: r#"function() {
                    this.style.outline = '2px solid red';
                    this.style.outlineOffset = '2px';
                    const el = this;
                    setTimeout(() => {
                        el.style.outline = '';
                        el.style.outlineOffset = '';
                    }, 3000);
                }"#
                .to_string(),
                object_id: Some(object_id),
                arguments: None,
                return_by_value: Some(true),
                await_promise: Some(false),
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

pub async fn tap_touch(
    client: &CdpClient,
    session_id: &str,
    ref_map: &RefMap,
    selector_or_ref: &str,
) -> Result<(), String> {
    let (x, y) = resolve_element_center(client, session_id, ref_map, selector_or_ref).await?;

    client
        .send_command(
            "Input.dispatchTouchEvent",
            Some(serde_json::json!({
                "type": "touchStart",
                "touchPoints": [{ "x": x, "y": y }],
            })),
            Some(session_id),
        )
        .await?;

    client
        .send_command(
            "Input.dispatchTouchEvent",
            Some(serde_json::json!({
                "type": "touchEnd",
                "touchPoints": [],
            })),
            Some(session_id),
        )
        .await?;

    Ok(())
}

async fn dispatch_click(
    client: &CdpClient,
    session_id: &str,
    x: f64,
    y: f64,
    button: &str,
    click_count: i32,
) -> Result<(), String> {
    // Move
    client
        .send_command_typed::<_, Value>(
            "Input.dispatchMouseEvent",
            &DispatchMouseEventParams {
                event_type: "mouseMoved".to_string(),
                x,
                y,
                button: None,
                buttons: None,
                click_count: None,
                delta_x: None,
                delta_y: None,
                modifiers: None,
            },
            Some(session_id),
        )
        .await?;

    let button_value = match button {
        "right" => 2,
        "middle" => 4,
        _ => 1,
    };

    // Press
    client
        .send_command_typed::<_, Value>(
            "Input.dispatchMouseEvent",
            &DispatchMouseEventParams {
                event_type: "mousePressed".to_string(),
                x,
                y,
                button: Some(button.to_string()),
                buttons: Some(button_value),
                click_count: Some(click_count),
                delta_x: None,
                delta_y: None,
                modifiers: None,
            },
            Some(session_id),
        )
        .await?;

    // Release
    client
        .send_command_typed::<_, Value>(
            "Input.dispatchMouseEvent",
            &DispatchMouseEventParams {
                event_type: "mouseReleased".to_string(),
                x,
                y,
                button: Some(button.to_string()),
                buttons: Some(0),
                click_count: Some(click_count),
                delta_x: None,
                delta_y: None,
                modifiers: None,
            },
            Some(session_id),
        )
        .await?;

    Ok(())
}

/// Maps a DOM KeyboardEvent.code value to a Windows virtual key code.
///
/// Covers editing/whitespace, navigation, modifiers, function keys,
/// punctuation, letters (KeyA..KeyZ), and digits (Digit0..Digit9).
/// Returns 0 for unrecognized codes.
pub fn code_to_virtual_key_code(code: &str) -> i32 {
    match code {
        // Editing & whitespace
        "Backspace" => 8,
        "Tab" => 9,
        "Enter" | "NumpadEnter" => 13,
        "Escape" => 27,
        "Space" => 32,
        "Delete" => 46,

        // Navigation
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Home" => 36,
        "End" => 35,
        "PageUp" => 33,
        "PageDown" => 34,
        "Insert" => 45,

        // Modifiers
        "ShiftLeft" | "ShiftRight" => 16,
        "ControlLeft" | "ControlRight" => 17,
        "AltLeft" | "AltRight" => 18,
        "MetaLeft" => 91,
        "MetaRight" => 93,
        "CapsLock" => 20,
        "NumLock" => 144,
        "ScrollLock" => 145,

        // Function keys
        "F1" => 112,
        "F2" => 113,
        "F3" => 114,
        "F4" => 115,
        "F5" => 116,
        "F6" => 117,
        "F7" => 118,
        "F8" => 119,
        "F9" => 120,
        "F10" => 121,
        "F11" => 122,
        "F12" => 123,

        // Punctuation
        "Minus" => 189,
        "Equal" => 187,
        "BracketLeft" => 219,
        "BracketRight" => 221,
        "Backslash" => 220,
        "Semicolon" => 186,
        "Quote" => 222,
        "Comma" => 188,
        "Period" => 190,
        "Slash" => 191,
        "Backquote" => 192,

        // Letters: KeyA(65)..KeyZ(90)
        _ if code.starts_with("Key") && code.len() == 4 => {
            let ch = code.as_bytes()[3];
            if ch.is_ascii_uppercase() {
                ch as i32
            } else {
                0
            }
        }

        // Digits: Digit0(48)..Digit9(57)
        _ if code.starts_with("Digit") && code.len() == 6 => {
            let ch = code.as_bytes()[5];
            if ch.is_ascii_digit() {
                ch as i32
            } else {
                0
            }
        }

        _ => 0,
    }
}

fn char_to_key_info(ch: char) -> (String, String, i32) {
    match ch {
        '\n' | '\r' => ("Enter".to_string(), "Enter".to_string(), 13),
        '\t' => ("Tab".to_string(), "Tab".to_string(), 9),
        ' ' => (" ".to_string(), "Space".to_string(), 32),
        _ => {
            let key = ch.to_string();
            let code = if ch.is_ascii_alphabetic() {
                format!("Key{}", ch.to_uppercase())
            } else if ch.is_ascii_digit() {
                format!("Digit{}", ch)
            } else {
                String::new()
            };
            let key_code = ch as i32;
            (key, code, key_code)
        }
    }
}

fn named_key_info(key: &str) -> (String, String, i32) {
    match key.to_lowercase().as_str() {
        "enter" | "return" => ("Enter".to_string(), "Enter".to_string(), 13),
        "tab" => ("Tab".to_string(), "Tab".to_string(), 9),
        "escape" | "esc" => ("Escape".to_string(), "Escape".to_string(), 27),
        "backspace" => ("Backspace".to_string(), "Backspace".to_string(), 8),
        "delete" => ("Delete".to_string(), "Delete".to_string(), 46),
        "arrowup" | "up" => ("ArrowUp".to_string(), "ArrowUp".to_string(), 38),
        "arrowdown" | "down" => ("ArrowDown".to_string(), "ArrowDown".to_string(), 40),
        "arrowleft" | "left" => ("ArrowLeft".to_string(), "ArrowLeft".to_string(), 37),
        "arrowright" | "right" => ("ArrowRight".to_string(), "ArrowRight".to_string(), 39),
        "home" => ("Home".to_string(), "Home".to_string(), 36),
        "end" => ("End".to_string(), "End".to_string(), 35),
        "pageup" => ("PageUp".to_string(), "PageUp".to_string(), 33),
        "pagedown" => ("PageDown".to_string(), "PageDown".to_string(), 34),
        "space" | " " => (" ".to_string(), "Space".to_string(), 32),
        _ => {
            if key.len() == 1 {
                let ch = key.chars().next().unwrap();
                char_to_key_info(ch)
            } else {
                (key.to_string(), key.to_string(), 0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_and_whitespace_keys() {
        assert_eq!(code_to_virtual_key_code("Backspace"), 8);
        assert_eq!(code_to_virtual_key_code("Tab"), 9);
        assert_eq!(code_to_virtual_key_code("Enter"), 13);
        assert_eq!(code_to_virtual_key_code("NumpadEnter"), 13);
        assert_eq!(code_to_virtual_key_code("Escape"), 27);
        assert_eq!(code_to_virtual_key_code("Space"), 32);
        assert_eq!(code_to_virtual_key_code("Delete"), 46);
    }

    #[test]
    fn navigation_keys() {
        assert_eq!(code_to_virtual_key_code("ArrowLeft"), 37);
        assert_eq!(code_to_virtual_key_code("ArrowUp"), 38);
        assert_eq!(code_to_virtual_key_code("ArrowRight"), 39);
        assert_eq!(code_to_virtual_key_code("ArrowDown"), 40);
        assert_eq!(code_to_virtual_key_code("Home"), 36);
        assert_eq!(code_to_virtual_key_code("End"), 35);
        assert_eq!(code_to_virtual_key_code("PageUp"), 33);
        assert_eq!(code_to_virtual_key_code("PageDown"), 34);
        assert_eq!(code_to_virtual_key_code("Insert"), 45);
    }

    #[test]
    fn modifier_keys() {
        assert_eq!(code_to_virtual_key_code("ShiftLeft"), 16);
        assert_eq!(code_to_virtual_key_code("ShiftRight"), 16);
        assert_eq!(code_to_virtual_key_code("ControlLeft"), 17);
        assert_eq!(code_to_virtual_key_code("ControlRight"), 17);
        assert_eq!(code_to_virtual_key_code("AltLeft"), 18);
        assert_eq!(code_to_virtual_key_code("AltRight"), 18);
        assert_eq!(code_to_virtual_key_code("MetaLeft"), 91);
        assert_eq!(code_to_virtual_key_code("MetaRight"), 93);
        assert_eq!(code_to_virtual_key_code("CapsLock"), 20);
        assert_eq!(code_to_virtual_key_code("NumLock"), 144);
        assert_eq!(code_to_virtual_key_code("ScrollLock"), 145);
    }

    #[test]
    fn function_keys() {
        for (i, expected) in (112..=123).enumerate() {
            let code = format!("F{}", i + 1);
            assert_eq!(code_to_virtual_key_code(&code), expected);
        }
    }

    #[test]
    fn punctuation_keys() {
        assert_eq!(code_to_virtual_key_code("Minus"), 189);
        assert_eq!(code_to_virtual_key_code("Equal"), 187);
        assert_eq!(code_to_virtual_key_code("BracketLeft"), 219);
        assert_eq!(code_to_virtual_key_code("BracketRight"), 221);
        assert_eq!(code_to_virtual_key_code("Backslash"), 220);
        assert_eq!(code_to_virtual_key_code("Semicolon"), 186);
        assert_eq!(code_to_virtual_key_code("Quote"), 222);
        assert_eq!(code_to_virtual_key_code("Comma"), 188);
        assert_eq!(code_to_virtual_key_code("Period"), 190);
        assert_eq!(code_to_virtual_key_code("Slash"), 191);
        assert_eq!(code_to_virtual_key_code("Backquote"), 192);
    }

    #[test]
    fn letter_keys() {
        assert_eq!(code_to_virtual_key_code("KeyA"), 65);
        assert_eq!(code_to_virtual_key_code("KeyZ"), 90);
        assert_eq!(code_to_virtual_key_code("KeyM"), 77);
    }

    #[test]
    fn digit_keys() {
        assert_eq!(code_to_virtual_key_code("Digit0"), 48);
        assert_eq!(code_to_virtual_key_code("Digit9"), 57);
        assert_eq!(code_to_virtual_key_code("Digit5"), 53);
    }

    #[test]
    fn unknown_code_returns_zero() {
        assert_eq!(code_to_virtual_key_code(""), 0);
        assert_eq!(code_to_virtual_key_code("UnknownKey"), 0);
        assert_eq!(code_to_virtual_key_code("Keya"), 0);
        assert_eq!(code_to_virtual_key_code("DigitX"), 0);
    }
}
