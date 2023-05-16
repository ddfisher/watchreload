use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{MessageEvent, WebSocket};

pub fn start_reload_listener() -> Result<bool, JsValue> {
    let Some(port) =  option_env!("WATCHRELOAD_PORT") else {
        return Ok(false);
    };

    let location = web_sys::window()
        .ok_or(JsValue::from("Could not access `window`"))?
        .location();

    let host = location.hostname()?;
    let websocket_protocol = if location.protocol()? == "https:" {
        "wss"
    } else {
        "ws"
    };

    let ws = WebSocket::new(&format!("{websocket_protocol}://{host}:{port}/"))?;
    let message_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            if txt == "reload" {
                location.reload().unwrap();
            }
        }
    });
    ws.set_onmessage(Some(message_callback.as_ref().unchecked_ref()));
    message_callback.forget();

    Ok(true)
}
