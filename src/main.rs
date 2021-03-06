#[macro_use]
extern crate glium;
extern crate gtk;
extern crate gdk;
extern crate libc;
extern crate epoxy;
extern crate shared_library;
extern crate glm;
extern crate image;
extern crate geometry_kernel;
extern crate glib;

use std::io::Cursor;
use std::ptr;
use std::cell::RefCell;
use std::rc::Rc;

use self::gtk::traits::*;
use self::gtk::Inhibit;
use self::gtk::{GLArea, Window};

use geometry_kernel::primitives::mesh::Mesh;

use std::fs::File;

use glium::Surface;

use self::shared_library::dynamic_library::DynamicLibrary;

// make moving clones into closures more convenient
macro_rules! clone {
    ($($n:ident),+; || $body:block) => (
        {
            $( let $n = $n.clone(); )+
            move || { $body }
        }
    );
    ($($n:ident),+; |$($p:ident),+| $body:block) => (
        {
            $( let $n = $n.clone(); )+
            move |$($p),+| { $body }
        }
    );
}

#[derive(Copy, Clone)]
struct VertexModel {
    position: [f32; 3],
    tex_coords: [f32; 2],
    normal: [f32; 3],
}
implement_vertex!(VertexModel, position, tex_coords, normal);

fn make_model(path : &str) -> Vec<VertexModel>{

    let mut maximum = 1.0f32;
    let mut model = vec![];
    let mut model_file = File::open(path).unwrap();
    let model_mesh = Mesh::read_stl(&mut model_file).unwrap();
    let triangle_indices = model_mesh.get_it_iterator();
    use geometry_kernel::primitives::number::NumberTrait;
    for i in triangle_indices {
        let triangle = model_mesh.get_triangle(i);
        let n = triangle.get_normal();
        let points = triangle.get_points();
        for p in points {
            let px = p.x.convert_to_f32();
            let py = p.y.convert_to_f32();
            let pz = p.z.convert_to_f32();
            let step_max = px.abs().max(py.max(pz.abs()));
            maximum = maximum.max(step_max);
            let nx = n.clone().x.convert_to_f32();
            let ny = n.clone().y.convert_to_f32();
            let nz = n.clone().z.convert_to_f32();
            model.push(VertexModel {
                position: [px, py, pz],
                tex_coords: [px, py], //tex_coords[index],
                normal: [nx, ny, nz]
            });
        }
    }
    let mut normalize_model = vec![];
    for vertex in &model {
        normalize_model.push(VertexModel {
            position: [vertex.position[0]/maximum,
                vertex.position[1]/maximum, vertex.position[2]/maximum],
            tex_coords: [(vertex.tex_coords[0]/maximum + 1.0)/2.0,
                (vertex.tex_coords[1]/maximum + 1.0)/2.0],
            normal: vertex.normal
        })
    }
    return normalize_model;
}

struct ModelState {
    model: std::vec::Vec<VertexModel>,
    is_render : bool,
}

fn main() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    let window = Window::new(gtk::WindowType::Toplevel);
    let glarea = GLArea::new();
    glarea.set_has_depth_buffer(true);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    epoxy::load_with(|s| {
        unsafe {
            match DynamicLibrary::open(None).unwrap().symbol(s) {
                Ok(v) => v,
                Err(_) => ptr::null(),
            }
        }
    });

    struct Backend {
        glarea: GLArea,
    }

    unsafe impl glium::backend::Backend for Backend {
        fn swap_buffers(&self) -> Result<(), glium::SwapBuffersError> {
            Ok(())
        }

        unsafe fn get_proc_address(&self, symbol: &str) -> *const std::os::raw::c_void {
            epoxy::get_proc_addr(symbol)
        }

        fn get_framebuffer_dimensions(&self) -> (u32, u32) {
            (self.glarea.get_allocated_width() as u32, self.glarea.get_allocated_height() as u32)
        }

        fn is_current(&self) -> bool {
            unsafe { self.make_current() };
            true
        }

        unsafe fn make_current(&self) {
            if self.glarea.get_realized() {
                self.glarea.make_current();
            }
        }
    }

    struct Facade {
        context: Rc<glium::backend::Context>,
    }

    impl glium::backend::Facade for Facade {
        fn get_context(&self) -> &Rc<glium::backend::Context> {
            &self.context
        }
    }

    impl Facade {
        fn draw(&self) -> glium::Frame {
            glium::Frame::new(self.context.clone(), self.context.get_framebuffer_dimensions())
        }
    }

    #[derive(Copy, Clone)]
    struct VertexLight {
        position: [f32; 3],
    }

    implement_vertex!(VertexLight, position);
    struct State {
        display: Facade,
        light_buffer: glium::VertexBuffer<VertexLight>,
        light_indices: glium::index::NoIndices,
        program_light: glium::program::Program,
        model_buffer: glium::VertexBuffer<VertexModel>,
        model_indices: glium::index::NoIndices,
        program_model: glium::program::Program,
        texture: glium::texture::Texture2d,
        tx: f32, ty: f32, tz: f32,
        rx: f32, ry: f32,
        scale: f32,
        is_draw: bool,
        is_light: bool, is_texture: bool,
        int: f32, amb: f32, diff: f32, spec: f32,
        back_color : gdk::RGBA, model_color : gdk::RGBA,
    }

    let state: Rc<RefCell<Option<State>>> = Rc::new(RefCell::new(None));

    glarea.connect_realize(clone!(glarea, state; |_widget| {
            let mut state = state.borrow_mut();

            let display = Facade {
                context: unsafe {
                    glium::backend::Context::new::<_, >(
                        Backend {
                            glarea: glarea.clone(),
                        }, true, Default::default())
                }.unwrap(),
            };

	let cube_light = vec![VertexLight {position: [-0.18, -0.18, -0.18]},
						  VertexLight {position: [-0.18, 0.18, -0.18]}, VertexLight {position: [0.18, -0.18, -0.18]},
						  VertexLight {position: [-0.18, 0.18, -0.18]},
						  VertexLight {position: [0.18, 0.18, -0.18]}, VertexLight {position: [0.18, -0.18, -0.18]},
						  VertexLight {position: [-0.18, -0.18, 0.18]},
						  VertexLight {position: [-0.18, 0.18, 0.18]}, VertexLight {position: [0.18, -0.18, 0.18]},
						  VertexLight {position: [-0.18, 0.18, 0.18]},
						  VertexLight {position: [0.18, 0.18, 0.18]}, VertexLight {position: [0.18, -0.18, 0.18]},
						  VertexLight {position: [0.18, -0.18, -0.18]},
						  VertexLight {position: [0.18, 0.18, -0.18]}, VertexLight {position: [0.18, -0.18, 0.18]},
						  VertexLight {position: [0.18, 0.18, -0.18]},
						  VertexLight {position: [0.18, 0.18, 0.18]}, VertexLight {position: [0.18, -0.18, 0.18]},
						  VertexLight {position: [-0.18, -0.18, -0.18]},
						  VertexLight {position: [-0.18, 0.18, -0.18]}, VertexLight {position: [-0.18, -0.18, 0.18]},
						  VertexLight {position: [-0.18, 0.18, -0.18]},
						  VertexLight {position: [-0.18, 0.18, 0.18]}, VertexLight {position: [-0.18, -0.18, 0.18]},
						  VertexLight {position: [-0.18, 0.18, -0.18]},
						  VertexLight {position: [0.18, 0.18, 0.18]}, VertexLight {position: [0.18, 0.18, -0.18]},
						  VertexLight {position: [-0.18, 0.18, 0.18]},
						  VertexLight {position: [0.18, 0.18, 0.18]}, VertexLight {position: [0.18, 0.18, -0.18]},
						  VertexLight {position: [-0.18, -0.18, -0.18]},
						  VertexLight {position: [-0.18, -0.18, 0.18]}, VertexLight {position: [0.18, -0.18, -0.18]},
						  VertexLight {position: [-0.18, -0.18, 0.18]},
						  VertexLight {position: [0.18, -0.18, 0.18]}, VertexLight {position: [0.18, -0.18, -0.18]}];

    let model = make_model("union.stl");
	let light_buffer = glium::VertexBuffer::new(&display, &cube_light).unwrap();
    let light_indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);
	let model_buffer = glium::VertexBuffer::new(&display, &model).unwrap();
    let model_indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);

    let vertex_shader_light = r#"
        #version 330
        in vec3 position;
        uniform mat4 modelMatrix, projectionMatrix;
        void main() {
            gl_Position = projectionMatrix * modelMatrix * vec4(position, 1.0);
        }
    "#;

    let fragment_shader_light = r#"
        #version 330
        out vec4 color;
        void main() {
            color = vec4(1.0, 1.0, 1.0, 1.0);
        }
    "#;

	 let vertex_shader_model = r#"
        #version 330
        in vec3 position;
		in vec2 tex_coords;
		in vec3 normal;
		out vec2 v_tex_coords;
		out vec3 v_normal;
		out vec3 v_position;
        uniform mat4 modelMatrix, projectionMatrix;
        void main() {
			v_tex_coords = tex_coords;
			v_normal = normalize(mat3(transpose(inverse(modelMatrix)))*normal);
			v_position = vec3(modelMatrix*vec4(position, 1.0));
            gl_Position = projectionMatrix * modelMatrix * vec4(position, 1.0);
        }
    "#;

    let fragment_shader_model = r#"
        #version 330
		in vec2 v_tex_coords;
		in vec3 v_normal;
		in vec3 v_position;
        out vec4 color;
        uniform sampler2D tex;
		uniform vec3 LightPosition;
		uniform vec3 LightIntensity;
		uniform vec3 MaterialKa;
		uniform vec3 MaterialKd;
		uniform float MaterialKs;
		uniform bool is_light;
		uniform bool is_texture;
		uniform vec4 model_color;
		out vec4 FragColor;
		void phongModel(vec3 pos, vec3 norm, out vec3 ambAndDiffspec) {
			vec3 ambient = LightIntensity*MaterialKa;
			vec3 lightDir = normalize(LightPosition - v_position);
			float diff = max(dot(v_normal, lightDir), 0.0);
			vec3 diffuse = LightIntensity*(diff * MaterialKd);
			vec3 viewPos = vec3(0.0, 0.0, 2.0);
			vec3 viewDir = normalize(viewPos - pos);
			vec3 r = reflect(-lightDir, norm);
			vec3 specular = vec3(pow(max(dot(r,viewDir), 0.0), 32)*MaterialKs*diff);
			ambAndDiffspec = ambient  + diffuse + specular;
		}
		void main() {
			vec3 ambAndDiffspec;
			vec4 texColor = texture(tex, v_tex_coords);
			phongModel(v_position, v_normal, ambAndDiffspec);
			if(is_light) {
				FragColor = vec4(ambAndDiffspec, 1.0) * model_color;
			} else {
				FragColor = model_color;
			}
			if(is_texture) {
				FragColor *= texColor;
			}
		}
    "#;

    let program_light = glium::Program::from_source(&display, vertex_shader_light, fragment_shader_light, None).unwrap();
	let program_model = glium::Program::from_source(&display, vertex_shader_model, fragment_shader_model, None).unwrap();
    let image = image::load(
        Cursor::new(&include_bytes!("t2.jpg")[..]),image::JPEG).unwrap().to_rgba();
    let image_dimensions = image.dimensions();
    let image = glium::texture::RawImage2d::from_raw_rgba_reversed(&image.into_raw(), image_dimensions);
    let texture = glium::texture::Texture2d::new(&display, image).unwrap();

    let tx = 0.0f32; let ty = 0.0f32; let tz = 0.0f32;
    let rx = 30.0f32; let ry = 45.0f32;
    let scale = 0.5f32;
    let is_draw = true;
    let is_light = true;
    let is_texture = true;
    let int = 1.0f32; let amb = 0.5f32; let diff = 1.0f32; let spec = 0.8f32;
    let back_color = gdk::RGBA{red : 0.0, green : 0.0, blue : 0.0, alpha : 1.0};
    let model_color = gdk::RGBA{red : 1.0, green : 1.0, blue : 1.0, alpha : 1.0};

    *state = Some(State {
        display: display,
        light_buffer: light_buffer,
        light_indices: light_indices,
        program_light: program_light,
        model_buffer: model_buffer,
        model_indices: model_indices,
        program_model: program_model,
        texture : texture,
        tx : tx, ty : ty, tz : tz,
        rx : rx, ry : ry, scale : scale,
        is_draw : is_draw,
        is_light : is_light, is_texture : is_texture,
        int : int, amb : amb, diff : diff, spec : spec,
        back_color : back_color, model_color : model_color,
         });
    }));
    let model = make_model("union.stl");
    let model_state: std::sync::Arc<std::sync::Mutex<ModelState>> = std::sync::Arc::new(std::sync::Mutex::new(ModelState{
        model : model, is_render : false,
    }));

    glarea.connect_unrealize(clone!(state; |_widget| {
            let mut state = state.borrow_mut();
            *state = None;
        }));

    glarea.connect_render(clone!(state, model_state; |_glarea, _glctx| {
            let mut state = state.borrow_mut();
            let state = state.as_mut().unwrap();
            state.model_buffer = glium::VertexBuffer::new(&state.display, &model_state.lock().unwrap().model).unwrap();
            let int = [state.int, state.int, state.int];
            let amb = [state.amb, state.amb, state.amb];
            let diff = [state.diff, state.diff, state.diff];
            let spec = state.spec;
            let back = state.back_color;
            let color = state.model_color;
            let mut target = state.display.draw();
            target.clear_color_and_depth((back.red as f32,
                back.green as f32 , back.blue as f32, back.alpha as f32), 1.0);
            let lm = glm::ext::look_at(glm::vec3(0.0, 0.0, 2.0), glm::vec3(0.0, 0.0, 0.0), glm::vec3(0.0, 1.0, 0.0));
            let tm_light0 = glm::ext::translate(&lm, glm::vec3(state.tx-0.5, state.ty+0.5, state.tz));
            let tm_light = glm::ext::scale(&tm_light0, glm::vec3(0.25, 0.25, 0.25));
            let tm_light = tm_light.as_array();
            let rmx = glm::ext::rotate(&lm, glm::radians(state.rx), glm::vec3(1.0, 0.0, 0.0));
            let rmy = glm::ext::rotate(&rmx, glm::radians(state.ry), glm::vec3(0.0, 1.0, 0.0));
            let sm = glm::ext::scale(&rmy, glm::vec3(state.scale, state.scale, state.scale));
            let sm = sm.as_array();
            let (w, h) = target.get_dimensions();
            let pmv  = glm::ext::perspective_rh(glm::radians(45.0f32),
                w as f32 / h as f32, 0.1f32, 100.0f32);
            let pmv = pmv.as_array();
            let pm = [
                *pmv[0].as_array(), *pmv[1].as_array(), *pmv[2].as_array(), *pmv[3].as_array(),
            ];
            let uniforms_light = uniform! {
                modelMatrix : [
                    *tm_light[0].as_array(), *tm_light[1].as_array(),
                    *tm_light[2].as_array(), *tm_light[3].as_array(),
                ],
                projectionMatrix: pm,
            };

            let uniforms_model = uniform! {
                modelMatrix : [
                    *sm[0].as_array(), *sm[1].as_array(), *sm[2].as_array(), *sm[3].as_array(),
                ],
                projectionMatrix: pm,
                tex: glium::uniforms::Sampler::wrap_function(glium::uniforms::Sampler::new
                    (&state.texture),glium::uniforms::SamplerWrapFunction::Repeat),
                LightIntensity: int,
                LightPosition: [state.tx-0.5, state.ty+0.5, 0.0f32],
                MaterialKa: amb,
                MaterialKd: diff,
                MaterialKs: spec,
                is_light: state.is_light,
                is_texture: state.is_texture,
                model_color: [color.red as f32, color.green as f32, color.blue as f32, color.alpha as f32],
            };
            let params = glium::DrawParameters {
                viewport: Some(glium::Rect {
                    left : 0, bottom : 0,  width : w, height : h
                }),
                .. Default::default()
            };
            if state.is_draw {
                if state.is_light {
                    target.draw(&state.light_buffer, &state.light_indices, &state.program_light,
                        &uniforms_light,&params).unwrap();
                }
                target.draw(&state.model_buffer, &state.model_indices, &state.program_model,
                    &uniforms_model,&params).unwrap();
            }
            target.finish().unwrap();
            Inhibit(false)
        }));
    window.set_title("GLArea Example");
    window.set_default_size(1400, 700);
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.set_homogeneous(false);
    let model_frame = gtk::Frame::new("Model");
    let model_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let lightning_frame = gtk::Frame::new("Lightning");
    let lightning_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    model_frame.add(&model_box);
    lightning_frame.add(&lightning_box);
    let colours_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    colours_box.set_homogeneous(true);
    colours_box.set_spacing(5);
    colours_box.set_border_width(3);
    let light_box1 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    light_box1.set_homogeneous(true);
    light_box1.set_spacing(5);
    light_box1.set_border_width(3);
    let light_box2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    light_box2.set_homogeneous(true);
    light_box2.set_spacing(5);
    light_box2.set_border_width(3);
    let c_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let b_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let int_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let amb_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let spec_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let diff_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    colours_box.add(&c_box);
    colours_box.add(&b_box);
    light_box2.add(&diff_box);
    light_box2.add(&spec_box);
    light_box1.add(&int_box);
    light_box1.add(&amb_box);
    lightning_box.add(&light_box1);
    lightning_box.add(&light_box2);
    model_frame.set_border_width(10);
    lightning_frame.set_border_width(10);
    let button_box = gtk::Box::new(gtk::Orientation::Vertical, 20);
    button_box.set_homogeneous(false);
    let area_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    area_box.set_homogeneous(false);
    area_box.set_hexpand(true);
    area_box.set_vexpand(true);
    let area_sub_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    area_sub_box.set_homogeneous(true);
    area_sub_box.set_hexpand(true);
    area_sub_box.set_vexpand(true);
    area_sub_box.set_border_width(5);
    area_box.add(&area_sub_box);
    let scale_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    area_box.add(&scale_box);
    scale_box.set_hexpand(true);
    scale_box.set_vexpand(false);
    let progress = gtk::ProgressBar::new();
    progress.set_text("MODEL LOADING");
    progress.set_fraction(0.0);
    progress.set_pulse_step(0.1);
    progress.set_show_text(true);
    let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    area_box.add(&progress_box);
    progress_box.set_hexpand(true);
    progress_box.set_vexpand(false);
    progress_box.add(&progress);
    progress_box.set_border_width(5);
    let light_button = gtk::CheckButton::new_with_label("enable");
    light_button.clicked();
    light_button.connect_clicked(clone!(state, glarea; |_light_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.is_light= !state.is_light;
        glarea.queue_render();
    }));
    lightning_box.add(&light_button);
    let int_button = gtk::SpinButton::new_with_range(0.0, 1.0, 0.05);
    int_button.set_value(1.0);
    int_button.connect_property_value_notify(clone!(state, glarea; |int_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.int = int_button.get_value() as f32;
        glarea.queue_render();
    }));
    let int_label = gtk::Label::new("intensivity");
    int_box.add(&int_label);
    int_box.add(&int_button);
    let amb_button = gtk::SpinButton::new_with_range(0.0, 1.0, 0.05);
    amb_button.set_value(0.5);
    amb_button.connect_property_value_notify(clone!(state, glarea; |amb_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.amb = amb_button.get_value() as f32;
        glarea.queue_render();
    }));
    let amb_label = gtk::Label::new("ambience");
    amb_box.add(&amb_label);
    amb_box.add(&amb_button);
    let diff_button = gtk::SpinButton::new_with_range(0.0, 1.0, 0.05);
    diff_button.set_value(1.0);
    diff_button.connect_property_value_notify(clone!(state, glarea; |diff_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.diff = diff_button.get_value() as f32;
        glarea.queue_render();
    }));
    let diff_label = gtk::Label::new("diffuse");
    diff_box.add(&diff_label);
    diff_box.add(&diff_button);
    let spec_button = gtk::SpinButton::new_with_range(0.0, 1.0, 0.05);
    spec_button.set_value(0.8);
    spec_button.connect_property_value_notify(clone!(state, glarea; |spec_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.spec = spec_button.get_value() as f32;
        glarea.queue_render();
    }));
    let spec_label = gtk::Label::new("specaluraty");
    spec_box.add(&spec_label);
    spec_box.add(&spec_button);
    let texture_button = gtk::CheckButton::new_with_label("");
    texture_button.clicked();
    texture_button.connect_clicked(clone!(state, glarea; |_texture_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.is_texture= !state.is_texture;
        glarea.queue_render();
    }));
    let scale_button = gtk::Scale::new_with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
    scale_button.set_value(0.5);
    scale_button.connect_value_changed(clone!(state, glarea; |scale_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.scale = scale_button.get_value() as f32;
        glarea.queue_render();
    }));
    let color_button = gtk::ColorButton::new_with_rgba(
        &gdk::RGBA{red : 1.0, green : 1.0, blue : 1.0, alpha : 1.0});
    color_button.set_title("model`s colour");
    color_button.connect_color_set(clone!(state, glarea; |color_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.model_color = color_button.get_rgba();
        glarea.queue_render();
    }));
    let color_label = gtk::Label::new("colour");
    c_box.add(&color_label);
    c_box.add(&color_button);
    let back_button = gtk::ColorButton::new_with_rgba(
        &gdk::RGBA{red : 0.0, green : 0.0, blue : 0.0, alpha : 1.0});
    back_button.set_title("glarea`s color");
    back_button.set_name("background");
    back_button.connect_color_set(clone!(state, glarea; |back_button| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        state.back_color = back_button.get_rgba();
        glarea.queue_render();
    }));
    let back_label = gtk::Label::new("back");
    b_box.add(&back_label);
    b_box.add(&back_button);
    let menu = gtk::Menu::new();
    let open = gtk::MenuItem::new_with_label("Open");
    let exit = gtk::MenuItem::new_with_label("Exit");
    let about = gtk::MenuItem::new_with_label("About");
    menu.append(&open);
    menu.append(&exit);
    menu.append(&about);
    about.connect_activate(clone!(window; |_about| {
        let dialog = gtk::MessageDialog::new(Some(&window), gtk::DialogFlags::empty(), gtk::MessageType::Info,
                                gtk::ButtonsType::None, "use WASD and 1234");
        dialog.run();
    }));
    exit.connect_activate(|_exit| {
        gtk::main_quit();
    });
    let open_button = gtk::FileChooserButton::new("load model", gtk::FileChooserAction::Open);
    open_button.set_width_chars(19);
    open_button.set_filename(std::path::Path::new("union.stl"));
    let open_dialog_filter = gtk::FileFilter::new();
    open_dialog_filter.add_pattern("*.stl");
    open_dialog_filter.set_name("*.stl");
    open_button.add_filter(&open_dialog_filter);
    open_button.connect_file_set(clone!(model_state, progress; |open_button| {
        use std::thread;
        progress.set_visible(true);
        let path = open_button.get_filename().unwrap();
        thread::spawn(clone!(path, model_state; || {
                model_state.lock().unwrap().is_render = true;
                let model = make_model(path.to_str().unwrap());
                if model_state.lock().unwrap().is_render {
                    model_state.lock().unwrap().model  = model;
                }
                model_state.lock().unwrap().is_render = false;
            }));
    }));
    let open_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let open_label = gtk::Label::new("STL-file");
    open_box.add(&open_label);
    open_box.add(&open_button);
    model_box.add(&open_box);
    model_box.add(&colours_box);
    let open_texture = gtk::FileChooserButton::new("load texture", gtk::FileChooserAction::Open);
    open_texture.set_width_chars(19);
    open_texture.set_filename(std::path::Path::new("t2.jpg"));
    let open_texture_filter = gtk::FileFilter::new();
    open_texture_filter.add_pattern("*.jpg");
    open_texture_filter.set_name("*.jpg");
    open_texture.add_filter(&open_texture_filter);
    open_texture.connect_file_set(clone!(state; |open_texture| {
        let mut state = state.borrow_mut();
            let state = state.as_mut().unwrap();
            let path = open_texture.get_filename().unwrap();
            let path_str = path.to_str().unwrap();
            let file = File::open(&path_str).unwrap();
            use std::io::Read;
            let mut reader = std::io::BufReader::new(file);
            let mut buf = Vec::new();
            let _length = reader.read_to_end(&mut buf);
            let image = image::load(
                Cursor::new(&buf.as_slice() as &std::convert::AsRef<[u8]>),image::JPEG).unwrap().to_rgba();
            let image_dimensions = image.dimensions();
            let image = glium::texture::RawImage2d::from_raw_rgba_reversed(&image.into_raw(), image_dimensions);
            state.texture = glium::texture::Texture2d::new(&state.display, image).unwrap();
    }));
    let texture_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let texture_label = gtk::Label::new("JPG-file");
    let texture_sub_box = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    texture_sub_box.add(&texture_button);
    texture_sub_box.add(&open_texture);
    texture_box.add(&texture_label);
    texture_box.add(&texture_sub_box);
    model_box.add(&texture_box);
    open.connect_activate(clone!(window, model_state, progress, open_button; |_open| {
        let open_dialog = gtk::FileChooserDialog::new(Some("load model"),
                                             Some(&window), gtk::FileChooserAction::Open);
        let open_dialog_filter = gtk::FileFilter::new();
        open_dialog_filter.add_pattern("*.stl");
        open_dialog_filter.set_name("*.stl");
        open_dialog.add_filter(&open_dialog_filter);
        open_dialog.connect_file_activated(clone!(model_state, progress, open_button; |open_dialog| {
        use std::thread;
        progress.set_visible(true);
        let path = open_dialog.get_filename().unwrap();
        thread::spawn(clone!(path, model_state; || {
                model_state.lock().unwrap().is_render = true;
                let model = make_model(path.to_str().unwrap());
                if model_state.lock().unwrap().is_render {
                    model_state.lock().unwrap().model  = model;
                }
                model_state.lock().unwrap().is_render = false;
            }));
        open_button.set_filename(std::path::Path::new(path.to_str().unwrap()));
            open_dialog.destroy();
        }));
        open_dialog.run();
    }));
    let menu_bar = gtk::MenuBar::new();
    let file = gtk::MenuItem::new_with_label("File");
    file.set_submenu(Some(&menu));
    menu_bar.append(&file);
    window.connect_key_press_event(clone!(state, glarea; |_window, key| {
        let keyval = gdk::EventKey::get_keyval(&key);
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        match keyval {
            gdk::enums::key::Escape => gtk::main_quit(),
            gdk::enums::key::a => if state.is_light { state.tx -= 0.1 },
            gdk::enums::key::d => if state.is_light { state.tx += 0.1 },
            gdk::enums::key::s => if state.is_light { state.ty -= 0.1 },
            gdk::enums::key::w => if state.is_light { state.ty += 0.1 },
            gdk::enums::key::f => if state.is_light { state.tz -= 0.1 },
            gdk::enums::key::r => if state.is_light { state.tz += 0.1 },
            gdk::enums::key::_4 => state.rx -= 5.0,
            gdk::enums::key::_3 => state.rx += 5.0,
            gdk::enums::key::_2 => state.ry += 5.0,
            gdk::enums::key::_1 => state.ry -= 5.0,
            _ => (),
        }
        glarea.queue_render();
        Inhibit(false)
    }));
    button_box.add(&model_frame);
    button_box.add(&lightning_frame);
    area_sub_box.add(&glarea);
    hbox.add(&button_box);
    scale_box.add(&scale_button);
    hbox.add(&area_box);
    button_box.pack_start(&menu_bar, false, false, 0);
    window.add(&hbox);
    window.show_all();
    glarea.set_visible(true);
    progress.set_visible(false);
    gtk::timeout_add(1, clone!(glarea; || {
        glarea.queue_render();
        return glib::Continue(true);
    }));
    gtk::timeout_add(1000, clone!(model_state; || {
        if !model_state.lock().unwrap().is_render {
            progress.set_visible(false);
        } else  {
            progress.pulse();
        }
        return glib::Continue(true);
    }));
    gtk::main();
}