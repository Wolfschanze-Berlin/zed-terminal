---
name: gpui-patterns
description: Quick reference for GPUI framework patterns used in zed-terminal — Entity lifecycle, Render/RenderOnce, actions, events, subscriptions, spawning async tasks, and element styling. Use this skill when writing any GPUI code, implementing Render or RenderOnce, creating entities, handling events or actions, working with focus, spawning async work, or when you need to understand how Window/Context/App types interact. Also use when you see errors related to GPUI types, borrowing issues with cx, or when element styling isn't working as expected.
---

# GPUI Patterns for zed-terminal

GPUI is a custom GPU-accelerated UI framework. It has no external docs — this reference is distilled from the codebase patterns and the project's .rules file.

## Context Types and When You Get Each One

| Context | You're inside... | Can do |
|---------|-----------------|--------|
| `&App` | Any read-only access | Read entities, read globals |
| `&mut App` | RenderOnce::render, event handlers | Read/write globals, new entities |
| `&mut Context<T>` | Entity<T>::update closure | Mutate T, emit events, spawn, notify, subscribe |
| `&mut AsyncApp` | cx.spawn closure | Update entities across await points |
| `&mut AsyncWindowContext` | cx.spawn_in closure | Same + window access across awaits |
| `&mut Window` | Render, event handlers | Focus, dispatch actions, input state |

The golden rule: `Context<T>` derefs to `App`, so any function taking `&App` also accepts `&Context<T>`.

## Entity Lifecycle

```rust
// Create
let entity: Entity<MyType> = cx.new(|cx: &mut Context<MyType>| MyType::new(cx));

// Read (immutable)
let value = entity.read(cx).some_field;

// Update (mutable) — the closure gets &mut T and &mut Context<T>
entity.update(cx, |this, cx| {
    this.some_field = new_value;
    cx.notify(); // trigger re-render
});

// Weak reference (breaks reference cycles, won't prevent drop)
let weak: WeakEntity<MyType> = entity.downgrade();
// weak.update() returns Result — fails if entity was dropped
```

**Trap**: Never update an entity inside its own update closure. This panics. Use `cx.spawn` to defer if needed.

## Render vs RenderOnce

Use `Render` for stateful views (panels, editors — things backed by an Entity):
```rust
impl Render for MyPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child("hello")
    }
}
```

Use `RenderOnce` for stateless UI components (buttons, labels — constructed and consumed):
```rust
#[derive(IntoElement)]
struct MyComponent { label: SharedString }

impl RenderOnce for MyComponent {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div().child(self.label)
    }
}
```

## Actions

```rust
// No-data actions
actions!(my_crate, [DoSomething, DoSomethingElse]);

// Data-carrying action
#[derive(Clone, PartialEq, Action)]
struct OpenFile { path: String }

// Register handler on element
div().on_action(cx.listener(|this, action: &DoSomething, window, cx| {
    // handle it
}))

// Dispatch programmatically
window.dispatch_action(DoSomething.boxed_clone(), cx);
```

## Events and Subscriptions

```rust
// Declare that MyPanel can emit MyEvent
impl EventEmitter<MyEvent> for MyPanel {}

// Emit inside an update
cx.emit(MyEvent::SomethingHappened);

// Subscribe from another entity's constructor
let subscription = cx.subscribe(&other_entity, |this, _emitter, event, cx| {
    match event {
        MyEvent::SomethingHappened => { /* react */ }
    }
});
// Store the subscription or it gets dropped and unsubscribed!
self._subscriptions.push(subscription);
```

## Async Work

```rust
// Foreground (UI thread) — can update entities
let task = cx.spawn(async move |this: WeakEntity<Self>, cx| {
    let result = cx.background_spawn(async { heavy_work() }).await;
    this.update(cx, |this, cx| {
        this.data = result;
        cx.notify();
    })?;
    anyhow::Ok(())
});
// Must store, await, or detach the task — dropping cancels it
task.detach_and_log_err(cx);

// Background (any thread) — no entity access
cx.background_spawn(async { compute_something() });
```

## Element Styling (Tailwind-like)

```rust
div()
    .size_full()          // width: 100%, height: 100%
    .bg(colors.panel_background)
    .border_1()
    .border_color(colors.border)
    .px_3()               // padding-x: 12px (3 * 4px)
    .py_1()               // padding-y: 4px
    .gap_1()              // flex gap: 4px
    .overflow_y_scroll()  // scrollable
    .cursor_pointer()

// Flex containers
h_flex()  // flex-direction: row
v_flex()  // flex-direction: column

// Conditional styling
div()
    .when(is_selected, |this| this.bg(colors.element_selected))
    .when_some(tooltip_text, |this, text| this.child(Tooltip::text(text)))
```

## Focus

```rust
// In your struct
struct MyPanel {
    focus_handle: FocusHandle,
}

// In constructor
focus_handle: cx.focus_handle(),

// In Render — MUST track focus for Panel trait to work
v_flex()
    .key_context("MyPanel")      // enables action dispatch for this context
    .track_focus(&self.focus_handle)
    .child(...)

// Focusable trait impl (required for Panel)
impl Focusable for MyPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
```

## Common Mistakes in This Codebase

1. **Using `unwrap()`** — Project rule: use `?` to propagate, `.log_err()` to ignore visibly, or `match`/`if let`.
2. **`let _ =` on fallible ops** — Project rule: never silently discard errors. Use `.log_err()` at minimum.
3. **Forgetting `cx.notify()`** — After mutating state that affects rendering, call `cx.notify()` or the UI won't update.
4. **Dropping subscriptions** — Store `Subscription` objects in a `Vec<Subscription>` field. If dropped, the subscription silently stops working.
5. **Using `smol::Timer`** in tests — Use `cx.background_executor().timer(duration).await` instead (project rule).
