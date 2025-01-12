use std::{io, thread};

use crate::Opt;
use termion::event::Key;
use termion::raw::IntoRawMode;
use tui::backend::TermionBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::Terminal;

use crate::repository;
use crate::widget::info;
use crate::widget::repo_entry;
use crate::widget::service_switcher;
use crate::widget::tag_list;

pub struct Ui {
    state: State,
    repo: crate::widget::repo_entry::RepoEntry,
    tags: crate::widget::tag_list::TagList,
    services: crate::widget::service_switcher::ServiceSwitcher,
    details: crate::widget::details::Details,
    info: crate::widget::info::Info,
}

#[derive(PartialEq, Clone)]
pub enum State {
    EditRepo,
    SelectTag,
    SelectService,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::EditRepo => write!(f, "Edit repository"),
            State::SelectTag => write!(f, "Select a tag"),
            State::SelectService => write!(f, "Select a image"),
        }
    }
}

impl std::iter::Iterator for State {
    type Item = Self;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            State::EditRepo => *self = State::SelectTag,
            State::SelectTag => *self = State::SelectService,
            State::SelectService => *self = State::EditRepo,
        }
        Some(self.clone())
    }
}

impl Ui {
    pub fn run(opt: &Opt) {
        let repo_id = opt.repo.as_deref();

        let mut ui = Ui {
            state: State::SelectService,
            repo: repo_entry::RepoEntry::new(repo_id),
            tags: tag_list::TagList::with_status("Tags are empty"),
            services: service_switcher::ServiceSwitcher::new(&opt.file).unwrap(),
            details: crate::widget::details::Details::new(),
            info: info::Info::new("Select image of edit Repository"),
        };

        if opt.repo.is_none() {
            ui.tags = tag_list::TagList::with_repo_name(ui.repo.get());
        }

        //setup tui
        let stdout = io::stdout().into_raw_mode().unwrap();
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend).unwrap();

        //setup input thread
        let receiver = super::spawn_stdin_channel();

        //core interaction loop
        'core: loop {
            //draw
            terminal
                .draw(|rect| {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            [
                                Constraint::Length(10),
                                Constraint::Length(3),
                                Constraint::Min(7),
                                Constraint::Length(2),
                            ]
                            .as_ref(),
                        )
                        .split(rect.size());

                    let (list, state) = ui.services.render(ui.state == State::SelectService);
                    rect.render_stateful_widget(list, chunks[0], state);
                    rect.render_widget(ui.repo.render(ui.state == State::EditRepo), chunks[1]);
                    let (list, state) = ui.tags.render(ui.state == State::SelectTag);
                    let more_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Min(15), Constraint::Length(28)].as_ref())
                        .split(chunks[2]);
                    rect.render_stateful_widget(list, more_chunks[0], state);
                    rect.render_widget(ui.details.render(), more_chunks[1]);
                    rect.render_widget(ui.info.render(), chunks[3]);
                })
                .unwrap();

            //handle input
            match receiver.try_recv() {
                Ok(Key::Ctrl('q')) => break 'core, //quit program without saving
                Ok(Key::Char('\t')) => {
                    ui.state.next();
                    ui.info.set_info(&ui.state);
                }
                Ok(Key::Ctrl('s')) => match ui.services.save() {
                    Err(e) => {
                        ui.info.set_info(&format!("{}", e));
                        continue;
                    }
                    Ok(_) => ui.info.set_text("Saved compose file"),
                },
                Ok(Key::Ctrl('r')) => {
                    ui.repo.confirm();
                    ui.tags = tag_list::TagList::with_repo_name(ui.repo.get());
                }
                Ok(Key::Char('\n')) => match ui.state {
                    State::EditRepo => {
                        ui.repo.confirm();
                        ui.tags = tag_list::TagList::with_repo_name(ui.repo.get());
                    }
                    State::SelectTag => {
                        let mut repo = ui.repo.get();
                        let tag = match ui.tags.get_selected() {
                            Err(tag_list::Error::NextPageSelected) => continue,
                            Err(e) => {
                                ui.info.set_info(&format!("{}", e));
                                continue;
                            }
                            Ok(tag) => tag,
                        };
                        repo.push(':');
                        repo.push_str(&tag);
                        ui.services.change_current_line(repo);
                    }
                    _ => (),
                },
                Ok(Key::Char(key)) => match ui.state {
                    State::SelectService => (),
                    State::EditRepo => {
                        ui.info.set_text("Editing Repository");
                        ui.repo.handle_input(Key::Char(key));
                    }
                    State::SelectTag => (),
                },
                Ok(Key::Backspace) => match ui.state {
                    State::SelectService => (),
                    State::EditRepo => {
                        ui.info.set_text("Editing Repository");
                        ui.repo.handle_input(Key::Backspace);
                    }
                    State::SelectTag => (),
                },
                Ok(Key::Up) => match ui.state {
                    State::SelectService if ui.services.find_previous_match() => {
                        match ui.services.extract_repo() {
                            Err(e) => ui.info.set_info(&format!("{}", e)),
                            Ok(s) => {
                                let repo = match repository::check_repo(&s) {
                                    Err(e) => {
                                        ui.info.set_info(&format!("{}", e));
                                        continue;
                                    }
                                    Ok(s) => s,
                                };
                                ui.repo.set(repo.to_string());
                                ui.tags = tag_list::TagList::with_repo_name(ui.repo.get());
                            }
                        }
                    }
                    State::SelectService => (),
                    State::EditRepo => (),
                    State::SelectTag => {
                        ui.tags.handle_input(Key::Up);
                        ui.details = ui.tags.create_detail_widget();
                    }
                },
                Ok(Key::Down) => match ui.state {
                    State::SelectService if ui.services.find_next_match() => {
                        match ui.services.extract_repo() {
                            Err(e) => ui.info.set_info(&format!("{}", e)),
                            Ok(s) => {
                                let repo = match repository::check_repo(&s) {
                                    Err(e) => {
                                        ui.info.set_info(&format!("{}", e));
                                        continue;
                                    }
                                    Ok(s) => s,
                                };
                                ui.repo.set(repo.to_string());
                                ui.tags = tag_list::TagList::with_repo_name(ui.repo.get());
                            }
                        }
                    }
                    State::SelectService => (),
                    State::EditRepo => (),
                    State::SelectTag => {
                        ui.tags.handle_input(Key::Down);
                        ui.details = ui.tags.create_detail_widget();
                    }
                },
                _ => (),
            }

            //sleep for 32ms (30 fps)
            thread::sleep(std::time::Duration::from_millis(32));
        }

        terminal.clear().unwrap();
    }
}
