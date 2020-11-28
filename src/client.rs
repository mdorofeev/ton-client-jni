extern crate ton_client;


extern crate jni;
extern crate lazy_static;

use jni::{JNIEnv};
use jni::sys::{jint};
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use self::ton_client::{ContextHandle, create_context, destroy_context, request, ResponseType};
use std::collections::HashMap;
use std::sync::{Mutex};


struct HandlerRepository {
    pub handlers: HashMap<u32, JniResultHandler>,
}

impl HandlerRepository {
    fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

lazy_static::lazy_static! {
    static ref HANDLERS: Mutex<HandlerRepository> = Mutex::new(HandlerRepository::new());
}


struct JniResultHandler {
    jvm: jni::JavaVM,
    handler: GlobalRef,
}

impl JniResultHandler {
    fn new(env: &JNIEnv, handler: JObject) -> JniResultHandler {
        JniResultHandler {
            jvm: env.get_java_vm().unwrap(),
            handler: env.new_global_ref(handler).unwrap(),
        }
    }
}

fn java_value<'a>(env: &JNIEnv<'a>, from: String) -> jni::objects::JValue<'a> {
    JValue::Object(env.new_string(from.as_str()).unwrap().into())
}

fn rust_string(env: &JNIEnv, from: JString) -> String {
    env.get_string(from).unwrap().into()
}

impl JniResultHandler {
    fn on_result(&self, result_json: String, error_json: String, response_type: i32) {
        let env = self.jvm.attach_current_thread().unwrap();
        let handler = self.handler.as_obj();
        let java_result_json = java_value(&env, result_json);
        let java_error_json = java_value(&env, error_json);
        let java_response_type = JValue::Int(response_type);

        env.call_method(
            handler,
            "invoke",
            "(Ljava/lang/String;Ljava/lang/String;I)V",
            &[java_result_json, java_error_json, java_response_type],
        ).unwrap();
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub unsafe extern fn Java_ton_sdk_TONSDKJsonApi_createContext<'a>(
    _env: JNIEnv<'a>,
    _class: JClass,
    config: JString
) -> JString<'a> {

    let response = create_context(rust_string(&_env, config));

    _env.new_string(response).unwrap()
}

#[allow(non_snake_case)]
#[no_mangle]
pub unsafe extern fn Java_ton_sdk_TONSDKJsonApi_destroyContext(
    _env: JNIEnv,
    _class: JClass,
    context: jint,
) {
    destroy_context(context as ContextHandle)
}

#[allow(non_snake_case)]
#[no_mangle]
pub unsafe extern fn Java_ton_sdk_TONSDKJsonApi_jsonRequestAsync(
    env: JNIEnv,
    _class: JClass,
    context: jint,
    _request_id: jint,
    method: JString,
    params_json: JString,
    on_result: JObject,
) {
    let mut handlers = HANDLERS.lock().unwrap();

    let handler = JniResultHandler::new(&env, on_result);

    let id = _request_id as u32;

    handlers.handlers.insert(
        id,
        handler
    );

    drop(handlers);

    request(context as ContextHandle,rust_string(&env, method),
            rust_string(&env, params_json), id, handler_callback);
}

fn handler_callback(request_id: u32, params_json: String, response_type: u32, finished: bool) {
    let mut handlers_repository = HANDLERS.lock().unwrap();

    let handler = match handlers_repository.handlers.get_mut(&request_id) {
        Some(handler) => handler,
        None => {
            println!("Handler not found: {}", request_id);
            return;
        }
    };

    if response_type == ResponseType::Success as u32 {
        handler.on_result(params_json, String::from(""), response_type as i32);
    } else if response_type == ResponseType::Error as u32 {
        handler.on_result(String::from(""), params_json, response_type as i32);
    } else if response_type == ResponseType::Nop as u32 {
    } else if response_type >= ResponseType::Custom as u32 {
        handler.on_result(params_json, String::from(""), response_type as i32);
    } else {
        panic!(format!("Unsupported response type: {}", response_type));
    }

    if finished {
        handlers_repository.handlers.remove(&request_id);
    }

    drop(handlers_repository);
}

