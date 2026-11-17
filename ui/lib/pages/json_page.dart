/// Raw JSON editor for the complete configuration document.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../strings.dart';
import '../widgets.dart';
import 'editor_scaffold.dart';

class JsonPage extends StatefulWidget {
  const JsonPage({
    super.key,
    required this.strings,
    required this.onNotice,
  });

  final Strings strings;
  final ValueChanged<String> onNotice;

  @override
  State<JsonPage> createState() => _JsonPageState();
}

class _JsonPageState extends State<JsonPage> {
  final _editor = TextEditingController();
  String? _loadedDraft;

  @override
  void dispose() {
    _editor.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final document = controller.document;
    final strings = widget.strings;
    final scheme = Theme.of(context).colorScheme;
    if (document == null) {
      return AgentUnavailable(strings: strings);
    }

    // Refill the editor when the document changed elsewhere, but never while
    // the user is typing raw JSON.
    if (_loadedDraft != document.draft && _editor.text != document.draft) {
      _loadedDraft = document.draft;
      _editor.text = document.draft;
    }

    final invalid = document.validationError != null;

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          EdgeGroup(
            children: [
              Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          document.path,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 11.5,
                            fontFamily: 'Menlo',
                            color: scheme.onSurfaceVariant,
                          ),
                        ),
                        const SizedBox(height: 3),
                        Row(
                          children: [
                            Icon(
                              invalid
                                  ? Icons.error_outline
                                  : Icons.check_circle_outline,
                              size: 13,
                              color: invalid ? scheme.error : scheme.primary,
                            ),
                            const SizedBox(width: 5),
                            Flexible(
                              child: Text(
                                invalid
                                    ? document.validationError!
                                    : document.isDirty
                                        ? strings.unsaved
                                        : strings.saved,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 12,
                                  color: invalid
                                      ? scheme.error
                                      : scheme.onSurfaceVariant,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                  if (!document.exists)
                    Padding(
                      padding: const EdgeInsets.only(right: 10),
                      child: Text(
                        strings.notCreated,
                        style: TextStyle(
                          fontSize: 11.5,
                          color: scheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  QuietAction(
                    label: strings.reload,
                    icon: Icons.restart_alt,
                    onPressed: () async {
                      final error = await controller.reloadDocument();
                      _loadedDraft = null;
                      widget.onNotice(error ?? strings.reload);
                    },
                  ),
                  QuietAction(
                    label: strings.validate,
                    icon: Icons.spellcheck,
                    onPressed: () async {
                      final error = await controller.validateNow();
                      widget.onNotice(error ?? strings.validate);
                    },
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 12),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: scheme.surfaceContainerLow,
                borderRadius: BorderRadius.circular(8),
                border: Border.all(color: scheme.outlineVariant),
              ),
              padding: const EdgeInsets.all(10),
              child: EdgeTextArea(
                controller: _editor,
                hint: '{ }',
                onChanged: controller.updateDraft,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
