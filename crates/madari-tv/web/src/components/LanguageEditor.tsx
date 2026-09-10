import {useEffect, useState} from 'react';
import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {Divider} from '@astryxdesign/core/Divider';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {List, ListItem} from '@astryxdesign/core/List';
import {Heading, Text} from '@astryxdesign/core/Text';
import {Token} from '@astryxdesign/core/Token';
import {VStack} from '@astryxdesign/core/VStack';
import {COMMON_LANGUAGES, languageName} from '../languages';
import {SettingsRow, type RowIcon} from './settings';

/** Ordered language priority: the first available language wins. */
export function LanguageEditor({
  label,
  description,
  icon,
  value,
  busy,
  onChange,
}: {
  label: string;
  description: string;
  icon: RowIcon;
  value: string[];
  busy: boolean;
  onChange: (next: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<string[]>(value);
  useEffect(() => {
    if (open) setDraft(value);
  }, [open, value]);
  const available = COMMON_LANGUAGES.filter((code) => !draft.includes(code));

  function move(from: number, to: number) {
    const next = [...draft];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    setDraft(next);
  }

  return (
    <>
      <SettingsRow
        title={label}
        description={description}
        icon={icon}
        control={
          <Button
            label={value.length ? 'Edit' : 'Add languages'}
            variant="secondary"
            size="sm"
            onClick={() => setOpen(true)}
          />
        }
        detail={
          value.length === 0 ? (
            <Text type="supporting" color="secondary">
              No preference · the video&rsquo;s default track is used
            </Text>
          ) : (
            <HStack gap={1} wrap="wrap">
              {value.map((code, index) => (
                <Token key={code} label={`${index + 1}. ${languageName(code)}`} size="sm" />
              ))}
            </HStack>
          )
        }
      />

      <Dialog isOpen={open} onOpenChange={(next) => !next && setOpen(false)} purpose="form" width={560}>
        <VStack gap={5}>
          <VStack gap={0}>
            <Heading level={3}>{label}</Heading>
            <Text color="secondary">First available language wins. Move languages to set their priority.</Text>
          </VStack>
          {draft.length === 0 ? (
            <Text type="supporting" color="secondary">
              No preference yet. Add a language below.
            </Text>
          ) : (
            <List hasDividers>
              {draft.map((code, index) => (
                <ListItem
                  key={code}
                  label={languageName(code)}
                  endContent={
                    <HStack gap={1} align="center">
                      <Button
                        label={`Move ${languageName(code)} up`}
                        variant="ghost"
                        size="sm"
                        isIconOnly
                        icon={<Icon icon="arrowUp" size="sm" />}
                        isDisabled={index === 0}
                        onClick={() => move(index, index - 1)}
                      />
                      <Button
                        label={`Move ${languageName(code)} down`}
                        variant="ghost"
                        size="sm"
                        isIconOnly
                        icon={<Icon icon="arrowDown" size="sm" />}
                        isDisabled={index === draft.length - 1}
                        onClick={() => move(index, index + 1)}
                      />
                      <Button
                        label="Remove"
                        variant="destructive"
                        size="sm"
                        onClick={() => setDraft(draft.filter((entry) => entry !== code))}
                      />
                    </HStack>
                  }
                />
              ))}
            </List>
          )}
          <Divider />
          <VStack gap={2}>
            <Text weight="semibold">Add a language</Text>
            <List hasDividers>
              {available.map((code) => (
                <ListItem
                  key={code}
                  label={languageName(code)}
                  endContent={<Button label="Add" size="sm" onClick={() => setDraft([...draft, code])} />}
                />
              ))}
            </List>
          </VStack>
          <HStack gap={2} justify="end">
            <Button label="Cancel" onClick={() => setOpen(false)} />
            <Button
              label="Save languages"
              variant="primary"
              isLoading={busy}
              onClick={() => {
                onChange(draft);
                setOpen(false);
              }}
            />
          </HStack>
        </VStack>
      </Dialog>
    </>
  );
}
