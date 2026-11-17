/// Shared scaffold for the three structured editors.
///
/// Resolvers, rule sets, and plugins all edit a list of JSON objects. They use
/// one master-detail layout: the list on the left is the only way to pick an
/// item, and the form on the right shows exactly that item. This removes the
/// duplicated "select + add + remove + fields" scaffolding that a per-page
/// layout would repeat three times.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';

/// Describes one entry in a [MasterList] for a list-backed editor.
class ListEntry {
  const ListEntry({
    required this.title,
    required this.subtitle,
    this.badge,
    this.dimmed = false,
    this.lines = const [],
  });

  final String title;
  final String subtitle;
  final String? badge;
  final bool dimmed;

  /// Extra label/value rows below the subtitle.
  final List<({String label, String value})> lines;
}

/// A structured editor page: header bar, master list, detail form.
class EditorScaffold extends StatelessWidget {
  const EditorScaffold({
    super.key,
    required this.strings,
    required this.listTitle,
    required this.addLabel,
    required this.removeLabel,
    required this.emptyLabel,
    required this.onAdd,
    required this.onRemove,
    required this.entries,
    required this.selected,
    required this.onSelect,
    required this.detail,
    this.header,
  });

  final Strings strings;
  final String listTitle;
  final String addLabel;
  final String removeLabel;
  final String emptyLabel;
  final VoidCallback onAdd;
  final VoidCallback onRemove;
  final List<ListEntry> entries;
  final int selected;
  final ValueChanged<int> onSelect;
  final Widget detail;
  final Widget? header;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (header != null) ...[
            header!,
            const SizedBox(height: 12),
          ],
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                MasterList(
                  title: listTitle,
                  addLabel: addLabel,
                  onAdd: onAdd,
                  itemCount: entries.length,
                  selected: selected,
                  onSelect: onSelect,
                  onRemove: onRemove,
                  removeLabel: removeLabel,
                  emptyLabel: emptyLabel,
                  itemBuilder: (context, index, isSelected) {
                    final entry = entries[index];
                    return MasterListItem(
                      title: entry.title,
                      subtitle: entry.subtitle,
                      badge: entry.badge,
                      dimmed: entry.dimmed,
                      lines: entry.lines,
                      selected: isSelected,
                      onTap: () => onSelect(index),
                    );
                  },
                ),
                const SizedBox(width: 12),
                Expanded(child: detail),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// Binds text controllers to the currently selected document entry.
///
/// The structured editors all follow the same cycle: the document changes, the
/// selection changes, or the user reloads, and the controllers must be refilled
/// exactly once per change. Centralising it keeps each page's `build` free of
/// per-field copy-paste and prevents the stale-controller bugs that come with
/// hand-written guards.
mixin FieldBinding<T extends StatefulWidget> on State<T> {
  final Map<String, TextEditingController> _controllers = {};
  Object? _signature;

  /// Returns the text controller for [key], creating it on first use.
  TextEditingController textField(String key) =>
      _controllers.putIfAbsent(key, TextEditingController.new);

  /// Refills every controller when [signature] changes. Call from `build`.
  void syncFields(Object signature, void Function() fill) {
    if (_signature == signature) {
      return;
    }
    _signature = signature;
    fill();
  }

  /// Forces the next [syncFields] call to refill, for edits that change the
  /// document in place without changing the signature.
  void invalidateFields() {
    _signature = null;
  }

  @override
  void dispose() {
    for (final controller in _controllers.values) {
      controller.dispose();
    }
    super.dispose();
  }
}

/// Signature helper: the inputs that make a form need a refill.
Object fieldSignature(List<Object?> parts) => Object.hashAll(parts);

/// Applies an edit through the controller and invalidates the local binding so
/// the form reflects any normalization the document performed.
void applyEdit(
  EdgeSteerController controller,
  FieldBinding<StatefulWidget> binding,
  void Function(ConfigDocument document) change,
) {
  controller.editDocument(change);
  binding.invalidateFields();
}

/// Placeholder shown when the agent cannot be reached, so the page does not
/// render an empty form that silently discards edits.
class AgentUnavailable extends StatelessWidget {
  const AgentUnavailable({super.key, required this.strings});

  final Strings strings;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        strings.agentUnavailable,
        style: TextStyle(
          fontSize: 13,
          color: Theme.of(context).colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}
