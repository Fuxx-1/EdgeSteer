/// Layout primitives.
///
/// The interface relies on structure instead of prose: a master list shows what
/// exists, the detail form shows what the selected item is, and short row labels
/// name each field. There is deliberately no room for explanatory paragraphs.
library;

import 'package:flutter/material.dart';

/// A bordered group with an optional header row. No subtitle: if a group needs
/// a sentence to be understood, the group is wrong.
class EdgeGroup extends StatelessWidget {
  const EdgeGroup({
    super.key,
    this.header,
    this.trailing,
    required this.children,
    this.padding = const EdgeInsets.all(14),
    this.scrollable = false,
  });

  final String? header;
  final Widget? trailing;
  final List<Widget> children;
  final EdgeInsets padding;
  final bool scrollable;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final content = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (header != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    header!.toUpperCase(),
                    style: TextStyle(
                      fontSize: 11,
                      letterSpacing: 0.6,
                      fontWeight: FontWeight.w600,
                      color: scheme.onSurfaceVariant,
                    ),
                  ),
                ),
                if (trailing != null) trailing!,
              ],
            ),
          ),
        ...children,
      ],
    );

    return Container(
      padding: padding,
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: scrollable ? SingleChildScrollView(child: content) : content,
    );
  }
}

/// One labelled control on a single line: fixed label column, flexible control.
class FormRow extends StatelessWidget {
  const FormRow({
    super.key,
    required this.label,
    required this.child,
    this.labelWidth = 116,
    this.hint,
    this.tight = false,
  });

  final String label;
  final Widget child;
  final double labelWidth;
  final String? hint;
  final bool tight;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: EdgeInsets.symmetric(vertical: tight ? 3 : 5),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          SizedBox(
            width: labelWidth,
            child: Text(
              label,
              style: TextStyle(fontSize: 12.5, color: scheme.onSurfaceVariant),
            ),
          ),
          Expanded(child: child),
          if (hint != null) ...[
            const SizedBox(width: 8),
            SizedBox(
              width: 148,
              child: Text(
                hint!,
                textAlign: TextAlign.right,
                style: TextStyle(
                  fontSize: 11.5,
                  color: scheme.onSurfaceVariant,
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// Two controls side by side, each with its own label column.
class FormRowPair extends StatelessWidget {
  const FormRowPair({super.key, required this.left, required this.right});

  final Widget left;
  final Widget right;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Expanded(child: left),
        const SizedBox(width: 16),
        Expanded(child: right),
      ],
    );
  }
}

/// Compact single-line text field sized for a form row.
class EdgeField extends StatelessWidget {
  const EdgeField({
    super.key,
    required this.controller,
    this.hint,
    this.numeric = false,
    this.onChanged,
    this.monospace = false,
    this.enabled = true,
    this.obscure = false,
  });

  final TextEditingController controller;
  final String? hint;
  final bool numeric;
  final ValueChanged<String>? onChanged;
  final bool monospace;
  final bool enabled;
  final bool obscure;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 32,
      child: TextField(
        controller: controller,
        enabled: enabled,
        obscureText: obscure,
        onChanged: onChanged,
        style: TextStyle(
          fontSize: 12.5,
          fontFamily: monospace ? 'Menlo' : null,
        ),
        keyboardType: numeric ? TextInputType.number : TextInputType.text,
        decoration: InputDecoration(
          hintText: hint,
          isDense: true,
          contentPadding:
              const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
          hintStyle: TextStyle(
            fontSize: 12,
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        ),
      ),
    );
  }
}

/// Compact dropdown sized for a form row.
class EdgeSelect<T> extends StatelessWidget {
  const EdgeSelect({
    super.key,
    required this.value,
    required this.items,
    required this.onChanged,
  });

  final T value;
  final List<({T value, String label})> items;
  final ValueChanged<T>? onChanged;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return SizedBox(
      height: 32,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: scheme.surfaceContainerHighest.withValues(alpha: 0.45),
          borderRadius: BorderRadius.circular(6),
          border: Border.all(color: scheme.outlineVariant),
        ),
        child: DropdownButtonHideUnderline(
          child: DropdownButton<T>(
            value: value,
            isExpanded: true,
            isDense: true,
            borderRadius: BorderRadius.circular(6),
            style: TextStyle(fontSize: 12.5, color: scheme.onSurface),
            onChanged: onChanged == null
                ? null
                : (next) {
                    if (next != null) {
                      onChanged!(next);
                    }
                  },
            items: [
              for (final item in items)
                DropdownMenuItem<T>(
                  value: item.value,
                  child: Text(item.label, overflow: TextOverflow.ellipsis),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Multi-line text area used by the JSON editor.
class EdgeTextArea extends StatelessWidget {
  const EdgeTextArea({
    super.key,
    required this.controller,
    required this.onChanged,
    this.hint,
  });

  final TextEditingController controller;
  final ValueChanged<String> onChanged;
  final String? hint;

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: controller,
      maxLines: null,
      expands: true,
      textAlignVertical: TextAlignVertical.top,
      style: const TextStyle(fontSize: 12, height: 1.5, fontFamily: 'Menlo'),
      decoration: InputDecoration(hintText: hint),
      onChanged: onChanged,
    );
  }
}

/// Master list of items with an add action. Selecting an item drives the detail
/// pane, so no "which one am I editing" dropdown is needed.
class MasterList extends StatelessWidget {
  const MasterList({
    super.key,
    required this.title,
    required this.addLabel,
    required this.onAdd,
    required this.itemCount,
    required this.itemBuilder,
    required this.selected,
    required this.onSelect,
    this.onRemove,
    this.removeLabel,
    this.emptyLabel,
    this.width = 236,
    this.footer,
  });

  final String title;
  final String addLabel;
  final VoidCallback onAdd;
  final int itemCount;
  final Widget Function(BuildContext context, int index, bool selected)
      itemBuilder;
  final int selected;
  final ValueChanged<int> onSelect;
  final VoidCallback? onRemove;
  final String? removeLabel;
  final String? emptyLabel;
  final double width;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return SizedBox(
      width: width,
      child: Container(
        decoration: BoxDecoration(
          color: scheme.surfaceContainerLow,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: scheme.outlineVariant),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 8, 6, 6),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      title,
                      style: TextStyle(
                        fontSize: 11,
                        letterSpacing: 0.6,
                        fontWeight: FontWeight.w600,
                        color: scheme.onSurfaceVariant,
                      ),
                    ),
                  ),
                  _TinyButton(
                    icon: Icons.add,
                    tooltip: addLabel,
                    onPressed: onAdd,
                  ),
                  if (onRemove != null)
                    _TinyButton(
                      icon: Icons.remove,
                      tooltip: removeLabel ?? '',
                      onPressed: itemCount == 0 ? null : onRemove,
                    ),
                ],
              ),
            ),
            Divider(height: 1, color: scheme.outlineVariant),
            Expanded(
              child: itemCount == 0
                  ? Center(
                      child: Padding(
                        padding: const EdgeInsets.all(12),
                        child: Text(
                          emptyLabel ?? '',
                          textAlign: TextAlign.center,
                          style: TextStyle(
                            fontSize: 12,
                            color: scheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                    )
                  : ListView.builder(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      itemCount: itemCount,
                      itemBuilder: (context, index) =>
                          itemBuilder(context, index, index == selected),
                    ),
            ),
            if (footer != null) ...[
              Divider(height: 1, color: scheme.outlineVariant),
              Padding(
                padding: const EdgeInsets.all(8),
                child: footer!,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// A row inside [MasterList].
class MasterListItem extends StatelessWidget {
  const MasterListItem({
    super.key,
    required this.title,
    required this.subtitle,
    required this.selected,
    required this.onTap,
    this.badge,
    this.dimmed = false,
  });

  final String title;
  final String subtitle;
  final bool selected;
  final VoidCallback onTap;
  final String? badge;
  final bool dimmed;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return InkWell(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
        color: selected ? scheme.primary.withValues(alpha: 0.14) : null,
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                      color: dimmed ? scheme.onSurfaceVariant : scheme.onSurface,
                    ),
                  ),
                  const SizedBox(height: 1),
                  Text(
                    subtitle,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 11,
                      fontFamily: 'Menlo',
                      color: scheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
            if (badge != null) ...[
              const SizedBox(width: 6),
              Text(
                badge!,
                style: TextStyle(
                  fontSize: 10,
                  fontWeight: FontWeight.w600,
                  color: selected ? scheme.primary : scheme.onSurfaceVariant,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _TinyButton extends StatelessWidget {
  const _TinyButton({
    required this.icon,
    required this.tooltip,
    required this.onPressed,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Tooltip(
      message: tooltip,
      waitDuration: const Duration(milliseconds: 500),
      child: InkResponse(
        onTap: onPressed,
        radius: 14,
        child: Padding(
          padding: const EdgeInsets.all(4),
          child: Icon(
            icon,
            size: 16,
            color: onPressed == null
                ? scheme.onSurfaceVariant.withValues(alpha: 0.4)
                : scheme.onSurfaceVariant,
          ),
        ),
      ),
    );
  }
}

/// Compact status readout: a labelled value with a state dot. Used instead of
/// big metric cards so related values sit on one line.
class StatusValue extends StatelessWidget {
  const StatusValue({
    super.key,
    required this.label,
    required this.value,
    this.active = false,
    this.monospace = false,
  });

  final String label;
  final String value;
  final bool active;
  final bool monospace;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 7,
          height: 7,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: active ? scheme.primary : scheme.outline,
          ),
        ),
        const SizedBox(width: 6),
        Text(
          label,
          style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
        ),
        const SizedBox(width: 6),
        Text(
          value,
          style: TextStyle(
            fontSize: 12.5,
            fontWeight: FontWeight.w600,
            fontFamily: monospace ? 'Menlo' : null,
            color: active ? scheme.primary : scheme.onSurface,
          ),
        ),
      ],
    );
  }
}

/// A labelled toggle on a form row.
class FormSwitch extends StatelessWidget {
  const FormSwitch({
    super.key,
    required this.value,
    required this.onChanged,
    this.enabled = true,
  });

  final bool value;
  final ValueChanged<bool>? onChanged;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerLeft,
      child: SizedBox(
        height: 32,
        child: Switch(
          value: value,
          onChanged: enabled ? onChanged : null,
          materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
        ),
      ),
    );
  }
}

/// A quiet inline text action.
class QuietAction extends StatelessWidget {
  const QuietAction({
    super.key,
    required this.label,
    required this.onPressed,
    this.icon,
    this.danger = false,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final color = danger ? scheme.error : scheme.onSurfaceVariant;
    final child = icon == null
        ? Text(label, style: TextStyle(fontSize: 12.5, color: color))
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 14, color: color),
              const SizedBox(width: 4),
              Text(label, style: TextStyle(fontSize: 12.5, color: color)),
            ],
          );
    return TextButton(
      onPressed: onPressed,
      style: TextButton.styleFrom(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        minimumSize: Size.zero,
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      child: child,
    );
  }
}
