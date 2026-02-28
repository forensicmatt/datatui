---
description: Add a new column operation (e.g. Clustering, PCA, LLM transformation)
---

This workflow guides the addition of a new data operation that can be performed on columns.

1. **Update Operation Metadata**
   - Add the new operation variant to `ColumnOperationKind` in `src/tui/components/column_operation_options_dialog.rs`.
   - Update `ColumnOperationKind::title()` and descriptions in `column_operations_dialog.rs`.
   - Add the specific configuration options to the `OperationOptions` enum.

2. **Extend the Options Dialog**
   - Update `ColumnOperationOptionsDialog` struct:
     - Add fields for the new operation's specific settings.
     - Add string buffers for numeric inputs (to support text editing).
   - Update `fields()`: Add the labels and input types for the new fields.
   - Update `field_value()`: Return the current value for display.
   - Update `num_string_for_field_mut()` or `text_field_mut()` to enable keyboard input for the new fields.
   - Update `try_apply()`: Validate and construct the `OperationOptions` variant.

3. **Handle Dispatch in App**
   - Open `src/tui/app.rs`.
   - Update `handle_column_operations_result()`: Add a match arm for the new operation.
   - Implement a new `dispatch_[operation_name]` method:
     - This method should spawn a background thread (to keep UI responsive).
     - Use cross-thread channels (`mpsc`) to send progress and results back to the main thread.
     - Access the `DataService` to fetch the source data.

4. **Implement Service Logic**
   - If the operation involves heavy computation or external APIs:
     - Add a new method to `DataService` or create a specialized service (e.g., `MLService`).
     - Ensure any database writes happen on the main thread to avoid DuckDB locking issues.

5. **Main Loop Integration**
   - Update `App::update()`:
     - Poll the new operation's result channel.
     - On success:
       - Refresh the `DataTable` (e.g., `table.reload_schema()`).
       - Notify the user of completion.
     - On progress: Update progress indicators.

6. **Verify**
   - Run `cargo check`.
   - Test the operation with sample data.
   - Check `datatui.log` for any errors or progress logs.
