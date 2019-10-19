use gameboy_core::emulator::traits::PixelMapper;
use gameboy_core::Button;
use gameboy_core::Color;
use gameboy_core::Controller;
use stdweb;
use stdweb::traits::IKeyboardEvent;
use stdweb::unstable::TryInto;
use stdweb::web::event::{KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseUpEvent};
use stdweb::web::html_element::CanvasElement;
use stdweb::web::{document, window, IEventTarget, IParentNode, TypedArray};
use webgl_rendering_context::*;

const VERTEX_SOURCE: &'static str = include_str!("shaders/vertex.glsl");
const FRAGMENT_SOURCE: &'static str = include_str!("shaders/fragment.glsl");
const VERTICIES: [f32; 12] = [
    1.0, 1.0, 0.0, 1.0, -1.0, 0.0, -1.0, -1.0, 0.0, -1.0, 1.0, 0.0,
];
const TEXTURE_COORDINATE: [f32; 8] = [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
const INDICIES: [u8; 6] = [0, 1, 3, 1, 2, 3];

type Gl = WebGLRenderingContext;

pub static mut CONTROLLER: Controller = Controller::new();

pub struct Screen {
    context: Gl,
    texture: WebGLTexture,
    shader_program: WebGLProgram,
    pixels: Vec<u8>,
}

impl Screen {
    pub fn new() -> Screen {
        stdweb::initialize();

        let up_btn = document().query_selector("#up-btn").unwrap().unwrap();
        let down_btn = document().query_selector("#down-btn").unwrap().unwrap();
        let left_btn = document().query_selector("#left-btn").unwrap().unwrap();
        let right_btn = document().query_selector("#right-btn").unwrap().unwrap();
        let a_btn = document().query_selector("#a-btn").unwrap().unwrap();
        let b_btn = document().query_selector("#b-btn").unwrap().unwrap();
        let start_btn = document().query_selector("#start-btn").unwrap().unwrap();
        let select_btn = document().query_selector("#select-btn").unwrap().unwrap();

        up_btn.add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::Up) });
        up_btn.add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::Up) });

        down_btn.add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::Down) });
        down_btn.add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::Down) });

        left_btn.add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::Left) });
        left_btn.add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::Left) });

        right_btn
            .add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::Right) });
        right_btn
            .add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::Right) });

        a_btn.add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::A) });
        a_btn.add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::A) });

        b_btn.add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::B) });
        b_btn.add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::B) });

        start_btn
            .add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::Start) });
        start_btn
            .add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::Start) });

        select_btn
            .add_event_listener(|_: MouseDownEvent| unsafe { CONTROLLER.press(Button::Select) });
        select_btn
            .add_event_listener(|_: MouseUpEvent| unsafe { CONTROLLER.release(Button::Select) });

        let canvas: CanvasElement = document()
            .query_selector("#canvas")
            .unwrap()
            .unwrap()
            .try_into()
            .unwrap();

        {
            window().add_event_listener(move |event: KeyDownEvent| unsafe {
                match event.key().as_ref() {
                    "ArrowUp" => CONTROLLER.press(Button::Up),
                    "ArrowDown" => CONTROLLER.press(Button::Down),
                    "ArrowLeft" => CONTROLLER.press(Button::Left),
                    "ArrowRight" => CONTROLLER.press(Button::Right),
                    "z" => CONTROLLER.press(Button::A),
                    "x" => CONTROLLER.press(Button::B),
                    "Enter" => CONTROLLER.press(Button::Select),
                    " " => CONTROLLER.press(Button::Start),
                    _ => (),
                }
            });
        }

        {
            window().add_event_listener(move |event: KeyUpEvent| unsafe {
                match event.key().as_ref() {
                    "ArrowUp" => CONTROLLER.release(Button::Up),
                    "ArrowDown" => CONTROLLER.release(Button::Down),
                    "ArrowLeft" => CONTROLLER.release(Button::Left),
                    "ArrowRight" => CONTROLLER.release(Button::Right),
                    "z" => CONTROLLER.release(Button::A),
                    "x" => CONTROLLER.release(Button::B),
                    "Enter" => CONTROLLER.release(Button::Select),
                    " " => CONTROLLER.release(Button::Start),
                    _ => (),
                }
            });
        }

        let context: Gl = canvas.get_context().unwrap();

        context.clear_color(1.0, 0.0, 0.0, 1.0);
        context.clear(Gl::COLOR_BUFFER_BIT);

        let verticies = TypedArray::<f32>::from(&VERTICIES[..]).buffer();
        let vertex_buffer = context.create_buffer().unwrap();
        context.bind_buffer(Gl::ARRAY_BUFFER, Some(&vertex_buffer));
        context.buffer_data_1(Gl::ARRAY_BUFFER, Some(&verticies), Gl::STATIC_DRAW);

        let textures = TypedArray::<f32>::from(&TEXTURE_COORDINATE[..]).buffer();
        let texture_buffer = context.create_buffer().unwrap();
        context.bind_buffer(Gl::ARRAY_BUFFER, Some(&texture_buffer));
        context.buffer_data_1(Gl::ARRAY_BUFFER, Some(&textures), Gl::STATIC_DRAW);

        let indicies = TypedArray::<u8>::from(&INDICIES[..]).buffer();
        let index_buffer = context.create_buffer().unwrap();
        context.bind_buffer(Gl::ELEMENT_ARRAY_BUFFER, Some(&index_buffer));
        context.buffer_data_1(Gl::ELEMENT_ARRAY_BUFFER, Some(&indicies), Gl::STATIC_DRAW);

        let vert_shader = context.create_shader(Gl::VERTEX_SHADER).unwrap();
        context.shader_source(&vert_shader, VERTEX_SOURCE);
        context.compile_shader(&vert_shader);

        let compiled = context.get_shader_parameter(&vert_shader, Gl::COMPILE_STATUS);

        if compiled == stdweb::Value::Bool(false) {
            let error = context.get_shader_info_log(&vert_shader);
            if let Some(e) = error {
                console!(log, e);
            }
        }

        let frag_shader = context.create_shader(Gl::FRAGMENT_SHADER).unwrap();
        context.shader_source(&frag_shader, FRAGMENT_SOURCE);
        context.compile_shader(&frag_shader);

        let compiled = context.get_shader_parameter(&frag_shader, Gl::COMPILE_STATUS);

        if compiled == stdweb::Value::Bool(false) {
            let error = context.get_shader_info_log(&frag_shader);
            if let Some(e) = error {
                console!(log, e);
            }
        }

        let shader_program = context.create_program().unwrap();
        context.attach_shader(&shader_program, &vert_shader);
        context.attach_shader(&shader_program, &frag_shader);
        context.link_program(&shader_program);

        context.bind_buffer(Gl::ARRAY_BUFFER, Some(&vertex_buffer));
        let pos_attr = context.get_attrib_location(&shader_program, "aPos") as u32;
        context.vertex_attrib_pointer(pos_attr, 3, Gl::FLOAT, false, 0, 0);
        context.enable_vertex_attrib_array(pos_attr);

        context.bind_buffer(Gl::ARRAY_BUFFER, Some(&texture_buffer));
        let tex_attr = context.get_attrib_location(&shader_program, "aTexCoord") as u32;
        context.vertex_attrib_pointer(tex_attr, 2, Gl::FLOAT, false, 0, 0);
        context.enable_vertex_attrib_array(tex_attr);

        let texture = context.create_texture().unwrap();
        context.bind_texture(Gl::TEXTURE_2D, Some(&texture));

        context.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::NEAREST as i32);
        context.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::NEAREST as i32);
        context.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE as i32);
        context.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);

        let pixels = vec![0; 144 * 160 * 3];

        Screen {
            context,
            texture,
            shader_program,
            pixels,
        }
    }

    pub fn render(&mut self) {
        self.context
            .bind_texture(Gl::TEXTURE_2D, Some(&self.texture));

        let pixels = &self.pixels[..];

        self.context.tex_image2_d(
            Gl::TEXTURE_2D,
            0,
            Gl::RGB as i32,
            160,
            144,
            0,
            Gl::RGB,
            Gl::UNSIGNED_BYTE,
            Some(pixels.as_ref()),
        );

        self.context.active_texture(Gl::TEXTURE0);

        self.context.use_program(Some(&self.shader_program));

        let screen_uniform = self
            .context
            .get_uniform_location(&self.shader_program, "screen")
            .unwrap();
        self.context.uniform1i(Some(&screen_uniform), 0);

        self.context
            .draw_elements(Gl::TRIANGLES, 6, Gl::UNSIGNED_BYTE, 0);
    }
}

impl PixelMapper for Screen {
    fn map_pixel(&mut self, pixel: usize, color: Color) {
        let color_bytes: [u8; 3] = match color {
            Color::White => [255, 255, 255],
            Color::LightGray => [178, 178, 178],
            Color::DarkGray => [102, 102, 102],
            Color::Black => [0, 0, 0],
        };

        for (i, byte) in color_bytes.iter().enumerate() {
            self.pixels[pixel * 3 + i] = *byte;
        }
    }

    fn get_pixel(&self, pixel: usize) -> Color {
        let offset = pixel * 3;
        match self.pixels[offset..offset + 3] {
            [255, 255, 255] => Color::White,
            [178, 178, 178] => Color::LightGray,
            [102, 102, 102] => Color::DarkGray,
            [0, 0, 0] => Color::Black,
            _ => panic!("this should never happen"),
        }
    }
}