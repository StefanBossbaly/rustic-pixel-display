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

### Simulator displaying the UpcomingArrivals render showing both SEPTA and Amtrak trains for 30th Street Station

![Train Arrivals](assets/train-arrivals-simulator.png)

### Raspberry Pi displaying an early version of the UpcomingArrivals render

![Train Arrivals](assets/train_arrivals.jpg)

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

## HTTP API

### Render API

Renders are constructed from a configuration provided to a Render Factory. Once loaded, their configuration can not be changed and
if a reconfiguration is required, the caller must construct a new render and remove the previous one. Once a render is created,
they can be given a layout slot to draw on by using the [Layout API](#layout-api). However it is completely valid for a render to
not be assigned a layout slot, therefore not draw anything on the output display. This can be useful for stateful Renders, which
will have background tasks running that update the render's state meaning they can't simply be loaded when the user requests them
since they would be missing important temporal context.

<details>
  <summary><code>GET</code> <code><b>/render/active</b></code> <code>(Returns the active renders current loaded)</code></summary>

##### Overview

Returns a list of loaded renders. These renders were successfully constructed via the `/factory/load/{factory_name}` endpoint and are
considered active. Active means that the render may have background thread/tasks running that can be used to keep track of the render
state, refresh backend API, etc. An active render might may or may not be currently displaying on the canvas.

##### Parameters

> None

##### Request Body

> None

##### Responses

> | http code | content-type       | response  |
> | --------- | ------------------ | --------- |
> | `200`     | `application/json` | See Below |

##### Response Body

> ```json
> [
>   {
>     "id": "UUID Serialize String",
>     "factory_name": "String",
>     "layout_slot": null or int
>   },
>   ...
> ]
> ```

##### Example cURL

> ```bash
>  curl -X GET http://localhost:8080/render/active
> ```

</details>

<details>
  <summary><code>DELETE</code> <code><b>/render/{render_id}</b></code> <code>(Unloads a render instance from memory)</code></summary>

##### Overview

Unloads and removes a render instance from the display (if applicable). Unloading a render instance will stop all background
threads/tasks and remove it from the layout. This operation is final, once a render is unloaded it must be recreated by providing
the same configuration to the `RenderFactory` that was used to create it.

##### Parameters

> | name        | type     | data type | description                                        |
> | ----------- | -------- | --------- | -------------------------------------------------- |
> | `render_id` | required | string    | The unique id provided when the render was created |

##### Request Body

> None

##### Responses

> | http code | content-type | response |
> | --------- | ------------ | -------- |
> | `204`     | None         | None     |
> | `404`     | None         | None     |

##### Example cURL

> ```bash
>  curl -X DELETE http://localhost:8080/render/{render_id}
> ```

</details>

### Factory API

Render Factories are compiled into the executable and are immutable. A caller can determine what factories are included in the program
by using the `/factory/discover` call. Their job is to read the configuration provided and construct a render that represents the
provided configuration parameters. Since each factory is different, the configuration schema will change from factory to factory. Because
of this, factories must also tell the caller the schema of the configuration they wish the user to provide them. This is accomplished in
the `/factory/details/{factory_name}` call. There may me multiple instances of renders that were created by the same Render Factory. Once
the render is crated, the Factory no longer plays a role it its lifetime management and the caller must use the [Render API](#render-api)
to interact with it.

<details>
  <summary><code>GET</code> <code><b>/factory/discover</b></code> <code>(Returns all available Render Factories)</code></summary>

##### Overview

Returns a list of RenderFactories that are served by this HTTP instance. Factories are compiled into the software executable and can not
be added after the compilation of the program.

##### Parameters

> None

##### Request Body

> None

##### Responses

> | http code | content-type       | response  |
> | --------- | ------------------ | --------- |
> | `200`     | `application/json` | See Below |

##### Response Body

> ```json
> [
>   {
>     "name": "String",
>     "description": "String",
>   },
>   ...
> ]
> ```

##### Example cURL

> ```bash
>  curl -X GET http://localhost:8080/factory/discover
> ```

</details>

<details>
  <summary><code>GET</code> <code><b>/factory/details/{factory_name}</b></code> <code>(Returns details about a specific render factory)(Under Construction)</code></summary>

##### Overview

Returns a list of details about a specific render factory. The returned object will contain the Render Factory's configuration schema.

##### Parameters

> None

##### Request Body

> None

##### Responses

> | http code | content-type       | response  |
> | --------- | ------------------ | --------- |
> | `200`     | `application/json` | See Below |
> | `404`     | None               | None      |

##### Response Body

> ```json
> [
>   {
>     "name": "String",
>     "description": "String",
>   },
>   ...
> ]
> ```

##### Example cURL

> ```bash
>  curl -X GET http://localhost:8080/factory/details/{factory_name}
> ```

</details>

<details>
  <summary><code>POST</code> <code><b>/factory/load/{factory_name}</b></code> <code>(Loads the render produced by the factory into memory)</code></summary>

##### Overview

Attempts to create a `Render` instance using the provided `RenderFactory`. The configuration must match the schema returned in the
`/factory/details/{factory_name}` endpoint. Once created, the `Render` must be referenced by using the UUID returned by this
function.

##### Parameters

> | name           | type     | data type | description                                                       |
> | -------------- | -------- | --------- | ----------------------------------------------------------------- |
> | `factory_name` | required | string    | The name of the factory described in the `/factory/discover` call |

##### Request Body

> Must be a serialized JSON object that matches the JSON schema specified by `/factory/details/{factory_name}` endpoint.
> The `RenderFactory` will parse it and attempt to build the associated render. This operation can fail and the `RenderFactory`
> will attempt to give a detailed error message so that the caller can attempt to fix the configuration.

##### Responses

> | http code | content-type       | response                                                        |
> | --------- | ------------------ | --------------------------------------------------------------- |
> | `200`     | `application/json` | `{id: "Serialized UUID of created Render instance"}`            |
> | `400`     | `application/json` | `{"description":"Render was not loaded","cause":"Bad Request"}` |
> | `404`     | None               | None                                                            |

##### Example cURL

> ```bash
>  curl -X POST -H "Content-Type: application/json" --data '{"station": "Downingtown"}' http://localhost:8080/factory/load/{render_name}
> ```

</details>

### Layout API (Under Construction)

Layouts allow multiple renders to output on the save LED Matrix Panel. Currently layouts are mutually exclusive, meaning that renders
can not draw overtop of each other. It is possible for the same Render instance to hold multiple layout slots.

<details>
  <summary><code>GET</code> <code><b>/layout/discover</b></code> <code>(Returns the supported layout configurations)</code></summary>

##### Overview

Returns a list of layout configurations that are supported by this instance.

##### Parameters

> None

##### Request Body

> None

##### Responses

> | http code | content-type       | response  |
> | --------- | ------------------ | --------- |
> | `200`     | `application/json` | See Below |

##### Response Body

> ```json
> [
>   {
>     name: "String",
>     items: integer,
>   }
> ]
> ```

##### Example cURL

> ```bash
>  curl -X GET http://localhost:8080/layout/discover
> ```

</details>

<details>
  <summary><code>GET</code> <code><b>/layout/active</b></code> <code>(Returns the active layout and associated renders)</code></summary>

##### Overview

Returns the active layout and which render (if any) is occupying each layout slot.

##### Parameters

> None

##### Request Body

> None

##### Responses

> | http code | content-type       | response  |
> | --------- | ------------------ | --------- |
> | `200`     | `application/json` | See Below |

##### Response Body

> ```json
> {
>   "name": "String",
>   "slots": [
>     {
>       "id": "UUID Serialize String",
>       "factory_name": "String"
>     },
>     {
>       "id": "UUID Serialize String",
>       "factory_name": "String"
>     },
>     null,
>     null
>   ]
> }
> ```

##### Example cURL

> ```bash
>  curl -X GET http://localhost:8080/layout/active
> ```

</details>

<details>
  <summary><code>POST</code> <code><b>/layout/config/{layout_config}</b></code> <code>(Configures the layout into a new configuration)</code></summary>
</details>

<details>
  <summary><code>POST</code> <code><b>/layout/select/{layout_slot}</b></code> <code>(Configures a render to draw in a layout slot)</code></summary>
</details>

<details>
  <summary><code>POST</code> <code><b>/layout/clear/{layout_slot}</b></code> <code>(Removes the current render in the layout slot)</code></summary>
</details>

## Authors

Stefan Bossbaly

## License

This project is licensed under the GPL-2.0 License - see the LICENSE file for details

## Acknowledgments

- [rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix)
- [rpi_led_panel (Rust implementation of rpi-rgb-led-matrix)](https://github.com/EmbersArc/rpi_led_panel)
- [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics)
- [embedded-layout](https://github.com/bugadani/embedded-layout)
