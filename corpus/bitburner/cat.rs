//! A cat that lives in its tail window: sits, walks along the bottom of the screen, jumps.
//! Port of BitburnerScripts' `cat.jsx`.

use react::{jsx, use_effect, use_ref, use_state, Node};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pose {
    Sit,
    Walk,
    Jump,
}

const STAND: &str = r"  /\_/\       \\
 (=o.o=)______//
  > ^ <        \
 (     )__     /
  \_|_/   \___/";
const WALK: &str = r"  /\_/\        |
 (=o.o=)______//
  > ^ <        \
 (     )__     /
 /_/|_/   \_|\_\";
const SIT: &str = r"  /\_/\
 (=o.o=)
  > ^ < \ \\
 (     ) \//
  \_|_/\__/";

fn art(pose: Pose, frame: u32) -> &'static str {
    match pose {
        Pose::Sit => SIT,
        Pose::Jump => WALK,
        Pose::Walk if (frame / 4) % 2 == 1 => WALK,
        Pose::Walk => STAND,
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("ALL");
    ns.ui.open_tail();
    ns.ui.resize_tail(160.0, 190.0);
    let floor = || js::window().inner_height - 210.0;

    let cat = {
        let ns = ns.clone();
        move || -> Node {
            let (pose, set_pose) = use_state(|| Pose::Sit);
            let (right, set_right) = use_state(|| true);
            let (frame, set_frame) = use_state(|| 0u32);

            // The interval reads this render's state through a ref, as `cat.jsx` does.
            let step = use_ref(0u32);
            step.set(frame);
            let tick = {
                let ns = ns.clone();
                let (set_pose, set_frame) = (set_pose.clone(), set_frame.clone());
                move || {
                    set_frame.update(|f| f + 1);
                    let Some(tail) = ns.get_running_script().and_then(|s| s.tail_properties) else { return };
                    match pose {
                        Pose::Sit if frame > 20 && js::Math::random() < 0.06 => {
                            set_pose.set(Pose::Walk);
                            set_frame.set(0);
                        }
                        Pose::Walk if frame > 20 && js::Math::random() < 0.06 => {
                            set_pose.set(Pose::Sit);
                            set_frame.set(0);
                        }
                        Pose::Walk => ns.ui.move_tail(tail.x + if right { 3.0 } else { -3.0 }, tail.y),
                        Pose::Jump if frame > 30 => set_pose.set(Pose::Sit),
                        Pose::Jump => {
                            let dy = ((f64::from(frame) / 3.0 - 5.0).powi(2) - 25.0) * 3.0;
                            ns.ui.move_tail(tail.x + if right { 1.5 } else { -1.5 }, floor() + dy);
                        }
                        _ => {}
                    }
                    if tail.x < 80.0 {
                        set_right.set(true);
                    }
                    if tail.x > js::window().inner_width - 300.0 {
                        set_right.set(false);
                    }
                }
            };
            let latest = use_ref(None::<Box<dyn Fn()>>);
            latest.set(Some(Box::new(tick)));
            use_effect(
                move || {
                    let latest = latest.clone();
                    let id = js::set_interval(
                        move || {
                            if let Some(t) = latest.get() {
                                t();
                            }
                        },
                        50.0,
                    );
                    move || js::clear_interval(id)
                },
                (),
            );
            let flip = if right { -1 } else { 1 };
            jsx! {
                <div style={format!("transform: scaleX({flip})")}>
                    <pre>{art(pose, frame)}</pre>
                </div>
            }
        }
    };
    ns.print_raw(jsx! { <cat /> });
    ns.ui.move_tail(100.0, floor());
}
