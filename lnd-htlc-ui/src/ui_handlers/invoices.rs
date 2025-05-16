use slint::{ModelRc, SharedString, VecModel, Weak, ComponentHandle};
use std::sync::{Arc, Mutex};
use std::thread;
use chrono;

use crate::MainWindow;
use crate::types::{AppState, Invoice};
use crate::invoice::{
    check_invoice, create_invoice, create_standard_invoice,
    pay_invoice, settle_invoice,
};

// Handler for checking invoice updates
pub fn check_invoice_updates_timer(app_state: &Arc<Mutex<AppState>>, window_weak: &Weak<MainWindow>) {
    let app_state = app_state.clone();
    let window_weak = window_weak.clone();
    
    // First, collect the pending invoices and their hashes
    let pending_hashes: Vec<String> = {
        if let Ok(state) = app_state.lock() {
            state.invoices
                .iter()
                .filter(|i| i.state == "PENDING")
                .map(|i| i.hash.to_string())
                .collect()
        } else {
            return;
        }
    };

    // Check each pending invoice
    let mut updates = Vec::new();
    for hash in pending_hashes {
        if let Ok(output) = check_invoice(&hash) {
            if output.contains("\"state\": \"ACCEPTED\"") {
                updates.push((hash.clone(), "ACCEPTED"));
            } else if output.contains("\"state\": \"SETTLED\"") {
                updates.push((hash.clone(), "SETTLED"));
            }
        }
    }

    // Apply updates in a single lock
    if !updates.is_empty() {
        let invoices_clone;
        {
            if let Ok(mut state) = app_state.lock() {
                for (hash, new_state) in updates {
                    if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                        invoice.state = SharedString::from(new_state);
                    }
                }
                
                state.status_message = "Updated invoice states".to_string();
                invoices_clone = state.invoices.clone();
            } else {
                return;
            }
        }
        
        if let Some(window) = window_weak.upgrade() {
            window.set_invoices(ModelRc::new(VecModel::from(
                invoices_clone,
            )));
            window.set_status_message(SharedString::from("Updated invoice states"));
        }
    }
}

// Initialize invoice handlers
pub fn init_invoice_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle create custom invoice button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_create_custom_invoice(move |preimage, amount, memo| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            let preimage = preimage.to_string();
            let amount = amount.to_string();
            let memo = memo.to_string();
            
            thread::spawn(move || {
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Creating custom invoice...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Creating custom invoice..."));
                }
                
                match create_invoice(preimage.clone(), amount.clone(), memo.clone()) {
                    Ok((payment_req, hash, _amt)) => {
                        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        let mut invoices_clone;
                        
                        // Parse the output and update state
                        {
                            if let Ok(mut state) = app_state.lock() {
                                let status_msg = format!("Created invoice with payment request: {}", payment_req);
                                state.status_message = status_msg;
                                
                                let new_invoice = Invoice {
                                    payment_request: SharedString::from(payment_req),
                                    hash: SharedString::from(hash),
                                    preimage: SharedString::from(preimage),
                                    memo: SharedString::from(memo),
                                    amount: SharedString::from(amount),
                                    created_at: SharedString::from(now),
                                    state: SharedString::from("PENDING"),
                                };
                                
                                state.invoices.push(new_invoice);
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(ModelRc::new(VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from("Created invoice"));
                            // Instead of directly setting active page, update the app state to trigger UI change
                            window.invoke_manage_channels();
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error creating invoice: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }

    // Handle pay invoice button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_pay_custom_invoice(move |bolt11| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            let bolt11 = bolt11.to_string();
            
            thread::spawn(move || {
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Paying invoice...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Paying invoice..."));
                }
                
                match pay_invoice(&bolt11) {
                    Ok(_) => {
                        let status_msg = "Payment successful";
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                            // Instead of directly setting active page, update the app state to trigger UI change
                            window.invoke_manage_channels();
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error paying invoice: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }

    // Handle claim invoice button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_claim_custom_invoice(move |hash, preimage| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            let hash = hash.to_string();
            let preimage = preimage.to_string();
            
            thread::spawn(move || {
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Settling invoice...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Settling invoice..."));
                }
                
                match settle_invoice(&hash, &preimage) {
                    Ok(_) => {
                        let status_msg = "Invoice settlement successful";
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.to_string();
                                
                                // Update invoice status
                                if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                                    invoice.state = SharedString::from("SETTLED");
                                }
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                            // Instead of directly setting active page, update the app state to trigger UI change
                            window.invoke_manage_channels();
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error settling invoice: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }

    // Handle standard invoice creation
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_create_standard_invoice(move |memo, amount| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            let memo = memo.to_string();
            let amount = amount as i32;
            
            thread::spawn(move || {
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Creating standard invoice...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Creating standard invoice..."));
                }
                
                match create_standard_invoice(memo.clone(), amount) {
                    Ok((payment_req, hash)) => {
                        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        let mut invoices_clone;
                        
                        // Parse the output and update state
                        {
                            if let Ok(mut state) = app_state.lock() {
                                let status_msg = "Created standard invoice";
                                state.status_message = status_msg.to_string();
                                
                                let new_invoice = Invoice {
                                    payment_request: SharedString::from(payment_req),
                                    hash: SharedString::from(hash),
                                    preimage: SharedString::from(""), // Standard invoices don't expose preimage
                                    memo: SharedString::from(memo),
                                    amount: SharedString::from(amount.to_string()),
                                    created_at: SharedString::from(now),
                                    state: SharedString::from("PENDING"),
                                };
                                
                                state.invoices.push(new_invoice);
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(ModelRc::new(VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from("Created standard invoice"));
                            // Instead of directly setting active page, update the app state to trigger UI change
                            window.invoke_manage_channels();
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error creating standard invoice: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }
} 