// Arena boss contract 1. Context is a copied JSON value; actions have no side effects.
// Rust advances the f32 phase timer and applies radial shots in the reference order.
fn boss(input) {
    if input.phase == 0 {
        if input.timerExpired {
            return #{action: "telegraph", duration: 0.8};
        }
        return #{action: "pursue", duration: 0.0};
    }
    if input.phase == 1 {
        if input.timerExpired {
            return #{action: "burst", duration: 1.0};
        }
        return #{action: "wait", duration: 0.0};
    }
    if input.timerExpired {
        return #{action: "recover", duration: 3.0};
    }
    #{action: "wait", duration: 0.0}
}
