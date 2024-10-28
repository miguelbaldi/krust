// Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
// this source code is governed by the GPL-3.0 license that can be
// found in the COPYING file.

use adw::prelude::*;

pub(crate) fn show_error_alert(parent: &impl IsA<gtk::Widget>, message: String) {
    let alert = adw::AlertDialog::builder()
        .heading_use_markup(true)
        .heading("<span foreground='red'><b>Error</b></span>")
        .title("Error")
        .body(message.as_str())
        .close_response("close")
        .default_response("close")
        .can_close(true)
        .receives_default(true)
        .build();
    alert.add_response("close", "Ok");
    alert.set_response_appearance("close", adw::ResponseAppearance::Destructive);
    alert.present(parent);
}

pub(crate) fn build_confirmation_alert(
    confirmation_label: String,
    message: String,
) -> adw::AlertDialog {
    let confirmation_alert = adw::AlertDialog::builder()
        .heading_use_markup(true)
        .heading("<span foreground='red'><b>Confirmation</b></span>")
        .title("Warning")
        .body(message.as_str())
        .close_response("confirm")
        .default_response("cancel")
        .can_close(true)
        .receives_default(true)
        .build();
    confirmation_alert.add_response("cancel", "Cancel");
    confirmation_alert.add_response("confirm", confirmation_label.as_str());
    confirmation_alert.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    confirmation_alert
}
