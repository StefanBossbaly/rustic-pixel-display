# Rustic Pixel Display

This project is currently a work in progress. The goal of this project is to build a driver for
RGB LED matrices that displays things like real time location, transit, stocks and various other
relevant information. The eventual goal is to have a plugin system that will allow other developers
to write their own renders.

# Goals

- Provide easy to understand APIs that allows developers to write their own renders
- Write clear examples that showcase the various features of this framework
- Have a functioning 1:1 simulator that allows developers to start their development without any hardware
- Have an optional embedded webserver to configure the hardware setup and settings for individual renders
- Have an Android app that allows for the configuration of hardware settings and individual renders

## Example Photos

![Train Arrivals](https://media.githubusercontent.com/media/StefanBossbaly/rustic-pixel-display/master/assets/train_arrivals.jpg)

# Raspberry Pi

When controlling hardware, a Raspberry Pi is required to control the LED matrix hardware display. Currently
I have only test this on Raspberry Pi 3/4, this might function on other Pis but there are no guarantees. To
get the most of your Pi, I recommend using a OS like (DietPi)[https://dietpi.com/]. If you are feeling adventurous,
I have written my own Yocto Distro to run this project called (yocto-raspberry-distro)[https://github.com/StefanBossbaly/yocto-raspberry-distro]
which strips out everything you don't need and is really meant for production environments. If you are not familiar
with the Yocto Project, I would not recommend going that route.

To compile for the Pi, you will need to compile this project and all of its dependencies for the `aarch64-unknown-linux-gnu`
target. When compiling on the Pi, it will default to that toolchain however when compiling on x64 machines you will
need to specify the target like so:

`cargo build --bin rpi --release --target=aarch64-unknown-linux-gnu`

For the Raspberry Pi, `rpi::LedDriver` is provided to drive the renders and handle any configuration changes with the
output panel.

```rust
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Construct your render(s) here

    let _led_driver = rpi::LedDriver::new(render, None)?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received!");
        }
    }

    Ok(())
}
```

## Simulator

In addition to running on Raspberry Pi hardware, the project can also be run on a local machine and output
to a native window using SDL2. Using the simulator allows developers to test changes on their local computer
and helps to reduce the turnaround time to test changes. To run the simulator binary using the following command:

`cargo run --bin simulator`

All renders will work with the simulator since the render trait use a trait bound for `DrawTarget`. For the simulator,
the `DrawTarget` trait will be implemented by the `SimulatorDisplay` struct.

```rust
// Change this for size of the output display to match your
// hardware configuration
const DISPLAY_SIZE: Size = Size {
    width: 128,
    height: 128,
};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let output_settings = OutputSettingsBuilder::new().scale(8).max_fps(60).build();
    let mut window = Window::new("Simulator", &output_settings);
    let mut canvas = SimulatorDisplay::<Rgb888>::new(DISPLAY_SIZE);

    // Construct your render(s) here

    'render_loop: loop {
        canvas
            .fill_solid(&Rectangle::new(Point::zero(), DISPLAY_SIZE), Rgb888::BLACK)
            .unwrap();

        // Call your render(s) and provide them with &mut canvas

        window.update(&canvas);

        for event in window.events() {
            if event == SimulatorEvent::Quit {
                break 'render_loop;
            }
        }
    }

    Ok(())
}
```

More information about the simulator and its dependencies can be found on the [embedded-graphics-simulator](https://crates.io/crates/embedded-graphics-simulator)
crate page.

## Authors

Stefan Bossbaly

## License

This project is licensed under the GPL-2.0 License - see the LICENSE file for details

## Acknowledgments

- [rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix)
- [rpi_led_panel (Rust implementation of rpi-rgb-led-matrix)](https://github.com/EmbersArc/rpi_led_panel)
- [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics)
- [embedded-layout](https://github.com/bugadani/embedded-layout)
