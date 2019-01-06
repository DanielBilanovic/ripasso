extern crate cursive;
extern crate env_logger;
extern crate ripasso;

use self::cursive::traits::*;
use self::cursive::views::{
    Dialog, EditView, LinearLayout, OnEventView, SelectView, TextArea, TextView,
};

use cursive::Cursive;

use self::cursive::direction::Orientation;
use self::cursive::event::{Event, Key};

extern crate clipboard;
use self::clipboard::{ClipboardContext, ClipboardProvider};

use ripasso::pass;
use std::process;
use std::process::Command;

use std::sync::Mutex;
fn main() {
    env_logger::init();

    // Load and watch all the passwords in the background
    let (_password_rx, passwords) = match pass::watch() {
        Ok(t) => t,
        Err(e) => {
            println!("Error {:?}", e);
            process::exit(1);
        }
    };

    let mut ui = Cursive::default();
    let rrx = Mutex::new(_password_rx);

    fn handleError(ui: &mut Cursive, err: Option<pass::Error>) -> () {
        if let Some(e) = err {
            let d = Dialog::around(TextView::new(format!("{:?}", e)))
                .dismiss_button("Ok")
                .title("Error");
            ui.add_layer(d);
        }
    }

    ui.cb_sink().send(Box::new(move |s: &mut Cursive| {
        let event = rrx.lock().unwrap().try_recv();
        if let pass::PasswordEvent::Error(e) = event.unwrap() {
            handleError(s, Some(e));
        }
    }));

    fn down(ui: &mut Cursive) -> () {
        ui.call_on_id("results", |l: &mut SelectView<pass::PasswordEntry>| {
            l.select_down(1);
        });
    }
    fn up(ui: &mut Cursive) -> () {
        ui.call_on_id("results", |l: &mut SelectView<pass::PasswordEntry>| {
            l.select_up(1);
        });
    }

    // Copy
    fn copy(ui: &mut Cursive) -> () {
        ui.call_on_id("results", |l: &mut SelectView<pass::PasswordEntry>| {
            let password = l.selection().unwrap().password().unwrap();
            let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
            ctx.set_contents(password.to_owned()).unwrap();
        });
    };
    ui.add_global_callback(Event::CtrlChar('y'), copy);
    ui.add_global_callback(Key::Enter, copy);

    // Movement
    ui.add_global_callback(Event::CtrlChar('n'), down);
    ui.add_global_callback(Event::CtrlChar('p'), up);

    // Query editing
    ui.add_global_callback(Event::CtrlChar('w'), |ui| {
        ui.call_on_id("searchbox", |e: &mut EditView| {
            e.set_content("");
        });
    });
    // VIM
    ui.add_global_callback(Event::CtrlChar('v'), |ui| {
        let r = ui.call_on_id(
            "results",
            |l: &mut SelectView<pass::PasswordEntry>| {
                let password_entry = l.selection().unwrap();
                let password = password_entry.password()?;
                let tmp_file_name: String = String::from_utf8_lossy(
                    &Command::new("/bin/mktemp")
                        .arg("/dev/shm/ripasso.XXXXXXXXXXXXX")
                        .output()
                        .expect("failed to create temporary file")
                        .stdout,
                )
                .into();
                use std::fs::File;
                use std::io::prelude::*;
                let mut tmp_file =
                    File::create(&tmp_file_name).expect("failed to open file");
                tmp_file
                    .write_all(&password.into_bytes())
                    .expect("failed to write to file");
                let status = Command::new("/bin/vim")
                    .arg(&tmp_file_name)
                    .status()
                    .expect("failed to execute process");
                println!("process exited with: {}", status);
                tmp_file =
                    File::open(&tmp_file_name).expect("failed to open file");
                let mut buffer = Vec::new();
                tmp_file.read_to_end(&mut buffer)?;
                password_entry.update(String::from_utf8(buffer)?)
            },
        );
        handleError(ui, r.unwrap().err())
    });

    // Editing
    ui.add_global_callback(Event::CtrlChar('o'), |ui| {
        let password_entry: pass::PasswordEntry = (*ui
            .call_on_id("results", |l: &mut SelectView<pass::PasswordEntry>| {
                l.selection().unwrap()
            })
            .unwrap())
        .clone();

        let password = password_entry.secret().unwrap();
        let d = Dialog::around(
            TextArea::new().content(password).with_id("editbox"),
        )
        .button("Edit", move |s| {
            let new_password = s
                .call_on_id("editbox", |e: &mut TextArea| {
                    e.get_content().to_string()
                })
                .unwrap();
            let r = password_entry.update(new_password);
            handleError(s, r.err())
        })
        .dismiss_button("Ok");

        ui.add_layer(d);
    });

    ui.load_toml(include_str!("../res/style.toml")).unwrap();
    let searchbox = EditView::new()
        .on_edit(move |ui, query, _| {
            let col = ui.screen_size().x;
            ui.call_on_id(
                "results",
                |l: &mut SelectView<pass::PasswordEntry>| {
                    let r = pass::search(&passwords, &String::from(query));
                    l.clear();
                    for p in &r {
                        let label = format!(
                            "{:2$}  {}",
                            p.name,
                            match p.updated {
                                Some(d) => format!("{}", d.format("%Y-%m-%d")),
                                None => "n/a".to_string(),
                            },
                            _ = col - 10 - 8, // Optimized for 80 cols
                        );
                        l.add_item(label, p.clone());
                    }
                },
            );
        })
        .with_id("searchbox")
        .fixed_width(72);

    // Override shortcuts on search box
    let searchbox = OnEventView::new(searchbox)
        .on_event(Key::Up, up)
        .on_event(Key::Down, down);

    let results = SelectView::<pass::PasswordEntry>::new()
        .with_id("results")
        .full_height();

    ui.add_layer(
        LinearLayout::new(Orientation::Vertical)
            .child(
                Dialog::around(
                    LinearLayout::new(Orientation::Vertical)
                        .child(searchbox)
                        .child(results)
                        .fixed_width(72),
                )
                .title("Ripasso"),
            )
            .child(
                LinearLayout::new(Orientation::Horizontal)
                    .child(TextView::new("CTRL-N: Next "))
                    .child(TextView::new("CTRL-P: Previous "))
                    .child(TextView::new("CTRL-Y: Copy "))
                    .child(TextView::new("CTRL-W: Clear "))
                    .child(TextView::new("CTRL-O: Open"))
                    .full_width(),
            ),
    );
    ui.run();
}
