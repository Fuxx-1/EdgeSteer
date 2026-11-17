/// Raw JSON editor for the complete configuration document.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../strings.dart';
import '../widgets.dart';

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
      return Center(child: Text(strings.agentUnavailable));
    }

    // Reload the editor when the document changes from a structured edit, but
    // never while the user is typing raw JSON.
    if (_loadedDraft != document.draft && _editor.text != document.draft) {
      _loadedDraft = document.draft;
      _editor.text = document.draft;
    }

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(32, 26, 32, 32),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          PageHeader(title: strings.jsonTitle, copy: strings.jsonCopy),
          const SizedBox(height: 20),
          EdgePanel(
            title: strings.configurationFile,
            subtitle: document.path,
            actions: ActionRow(
              children: [
                TextButton(
                  onPressed: () async {
                    final error = await controller.reloadDocument();
                    _loadedDraft = null;
                    widget.onNotice(
                      error ?? strings.configurationReloaded,
                    );
                  },
                  child: Text(strings.reload),
                ),
                OutlinedButton(
                  onPressed: () async {
                    final error = await controller.validateNow();
                    widget.onNotice(error ?? strings.configurationValid);
                  },
                  child: Text(strings.validate),
                ),
              ],
            ),
            children: [
              Row(
                children: [
                  Icon(
                    document.validationError == null
                        ? Icons.check_circle_outline
                        : Icons.error_outline,
                    size: 16,
                    color: document.validationError == null
                        ? scheme.primary
                        : scheme.error,
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      document.validationSummary,
                      style: TextStyle(
                        fontSize: 12.5,
                        color: document.validationError == null
                            ? scheme.onSurfaceVariant
                            : scheme.error,
                      ),
                    ),
                  ),
                  if (!document.exists)
                    Text(
                      strings.t(
                        '文件尚未创建',
                        'File not created yet',
                      ),
                      style: TextStyle(
                        fontSize: 12,
                        color: scheme.onSurfaceVariant,
                      ),
                    ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: 'JSON',
            padding: const EdgeInsets.all(12),
            children: [
              SizedBox(
                height: 470,
                child: TextField(
                  controller: _editor,
                  maxLines: null,
                  expands: true,
                  textAlignVertical: TextAlignVertical.top,
                  style: const TextStyle(
                    fontSize: 12,
                    height: 1.5,
                    fontFamily: 'Menlo',
                  ),
                  decoration: const InputDecoration(hintText: '{ }'),
                  onChanged: controller.updateDraft,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
