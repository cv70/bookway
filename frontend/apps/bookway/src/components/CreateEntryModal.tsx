import { ImagePlus, MapPin, Timer, X } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  Image,
  KeyboardAvoidingView,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import * as ImagePicker from 'expo-image-picker';

import { uploadImageAsset } from '../api/client';
import { colors } from '../theme';
import { CreateEntryInput, EntryMood, Journey } from '../types';

const moods: Array<{ key: EntryMood; label: string; color: string }> = [
  { key: 'clear', label: '清醒', color: colors.blue },
  { key: 'steady', label: '平稳', color: colors.evergreen },
  { key: 'energized', label: '有劲', color: colors.coral },
  { key: 'calm', label: '松弛', color: colors.plum },
  { key: 'tired', label: '疲惫', color: colors.gold },
];

type Props = {
  visible: boolean;
  actionId?: string;
  initialDurationMinutes?: number;
  journeyId?: string;
  journeys: Journey[];
  onClose: () => void;
  onSubmit: (entry: CreateEntryInput) => void;
};

export function CreateEntryModal({ visible, actionId, initialDurationMinutes, journeyId, journeys, onClose, onSubmit }: Props) {
  const [body, setBody] = useState('');
  const [mood, setMood] = useState<EntryMood>('steady');
  const [minutes, setMinutes] = useState('');
  const [quantity, setQuantity] = useState('');
  const [location, setLocation] = useState('');
  const [photoMediaId, setPhotoMediaId] = useState<string>();
  const [photoPreviewUri, setPhotoPreviewUri] = useState<string>();
  const [photoStatus, setPhotoStatus] = useState<string>();
  const [photoError, setPhotoError] = useState<string>();
  const [uploadingPhoto, setUploadingPhoto] = useState(false);
  const [publish, setPublish] = useState(false);

  useEffect(() => {
    if (!visible) return;
    setBody('');
    setMood('steady');
    setMinutes(initialDurationMinutes ? String(initialDurationMinutes) : '');
    setQuantity('');
    setLocation('');
    setPhotoMediaId(undefined);
    setPhotoPreviewUri(undefined);
    setPhotoStatus(undefined);
    setPhotoError(undefined);
    setUploadingPhoto(false);
    setPublish(false);
  }, [actionId, initialDurationMinutes, journeyId, visible]);

  const resolvedJourneyId = journeyId ?? journeys[0]?.id;
  const ready = body.trim().length > 0 && !uploadingPhoto;
  const pickPhoto = async () => {
    setPhotoError(undefined);
    const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
    if (!permission.granted) {
      setPhotoError('需要照片访问权限才能添加图片');
      return;
    }
    const result = await ImagePicker.launchImageLibraryAsync({
      mediaTypes: ['images'],
      allowsEditing: true,
      aspect: [4, 3],
      quality: 0.9,
    });
    if (result.canceled) return;
    const asset = result.assets[0];
    if (!asset?.uri) {
      setPhotoError('没有读取到所选图片');
      return;
    }
    setUploadingPhoto(true);
    setPhotoPreviewUri(asset.uri);
    setPhotoMediaId(undefined);
    setPhotoStatus('正在上传图片');
    try {
      const media = await uploadImageAsset(asset.uri, asset.mimeType);
      setPhotoMediaId(media.id);
      setPhotoStatus(media.status === 'ready' ? '图片已就绪' : '图片正在安全处理，行记会在处理完成后继续发布');
    } catch (error) {
      setPhotoPreviewUri(undefined);
      setPhotoStatus(undefined);
      setPhotoError(error instanceof Error ? error.message : '图片上传失败，请重试');
    } finally {
      setUploadingPhoto(false);
    }
  };
  const submit = () => {
    if (!ready) return;
    onSubmit({
      action_id: actionId,
      journey_id: resolvedJourneyId,
      body: body.trim(),
      mood,
      duration_minutes: minutes ? Number(minutes) : undefined,
      quantity: quantity.trim() || undefined,
      location: location.trim() || undefined,
      photo_media_id: photoMediaId,
      published: publish,
    });
  };

  return (
    <Modal animationType="slide" onRequestClose={onClose} transparent visible={visible}>
      <KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : undefined} style={styles.overlay}>
        <View style={styles.sheet}>
          <View style={styles.header}>
            <Text style={styles.title}>留下记录</Text>
            <Pressable accessibilityLabel="关闭记录" hitSlop={10} onPress={onClose} style={styles.close}><X color={colors.ink} size={22} /></Pressable>
          </View>
          <ScrollView contentContainerStyle={styles.form} keyboardShouldPersistTaps="handled" showsVerticalScrollIndicator={false}>
            <TextInput
              accessibilityLabel="记录内容"
              multiline
              onChangeText={setBody}
              placeholder="这一刻发生了什么？"
              placeholderTextColor={colors.faint}
              style={styles.bodyInput}
              textAlignVertical="top"
              value={body}
            />
            <View style={styles.field}>
              <Text style={styles.label}>此刻状态</Text>
              <View style={styles.moods}>
                {moods.map((item) => {
                  const selected = mood === item.key;
                  return (
                    <Pressable
                      accessibilityLabel={item.label}
                      accessibilityRole="radio"
                      accessibilityState={{ checked: selected }}
                      key={item.key}
                      onPress={() => setMood(item.key)}
                      style={styles.moodOption}
                    >
                      <View style={[styles.moodDot, { backgroundColor: item.color }, selected && styles.moodDotSelected]} />
                      <Text style={[styles.moodText, selected && styles.moodTextSelected]}>{item.label}</Text>
                    </Pressable>
                  );
                })}
              </View>
            </View>
            <View style={styles.metrics}>
              <View style={[styles.field, styles.metricField]}>
                <Text style={styles.label}>投入时长</Text>
                <View style={styles.inputWithIcon}><Timer color={colors.faint} size={16} /><TextInput keyboardType="number-pad" onChangeText={setMinutes} placeholder="分钟" placeholderTextColor={colors.faint} style={styles.compactInput} value={minutes} /></View>
              </View>
              <View style={[styles.field, styles.metricField]}>
                <Text style={styles.label}>数值记录</Text>
                <TextInput onChangeText={setQuantity} placeholder="如 3 km" placeholderTextColor={colors.faint} style={styles.compactTextInput} value={quantity} />
              </View>
            </View>
            {initialDurationMinutes ? <Text style={styles.timerHint}>已根据本次专注计时填入 {initialDurationMinutes} 分钟，你仍可手动调整。</Text> : null}
            <View style={styles.field}>
              <Text style={styles.label}>地点</Text>
              <View style={styles.inputWithIcon}><MapPin color={colors.faint} size={16} /><TextInput onChangeText={setLocation} placeholder="可选" placeholderTextColor={colors.faint} style={styles.compactInput} value={location} /></View>
            </View>
            <View style={styles.field}>
              <Text style={styles.label}>记录照片</Text>
              <Pressable accessibilityLabel="选择记录照片" disabled={uploadingPhoto} onPress={pickPhoto} style={({ pressed }) => [styles.photoPicker, pressed && !uploadingPhoto && styles.pressed, uploadingPhoto && styles.disabled]}>
                {photoPreviewUri ? <Image source={{ uri: photoPreviewUri }} style={styles.photoPreview} /> : <ImagePlus color={colors.evergreen} size={20} />}
                <View style={styles.photoPickerCopy}>
                  <Text style={styles.photoPickerTitle}>{uploadingPhoto ? '正在上传图片' : photoMediaId ? '更换照片' : '选择照片'}</Text>
                  <Text style={styles.photoPickerText}>{photoStatus ?? '上传后将进行安全处理'}</Text>
                </View>
              </Pressable>
              {photoError ? <Text style={styles.photoError}>{photoError}</Text> : null}
            </View>
            <View style={styles.publishRow}>
              <View style={styles.publishCopy}><Text style={styles.publishTitle}>发布为行记</Text><Text style={styles.publishText}>仅公开你填写的这条记录</Text></View>
              <Switch accessibilityLabel="发布为行记" onValueChange={setPublish} thumbColor={colors.surface} trackColor={{ false: colors.line, true: colors.evergreen }} value={publish} />
            </View>
          </ScrollView>
          <Pressable disabled={!ready} onPress={submit} style={({ pressed }) => [styles.submit, !ready && styles.disabled, pressed && ready && styles.pressed]}><Text style={styles.submitText}>{uploadingPhoto ? '图片上传中' : publish ? '发布行记' : '保存记录'}</Text></Pressable>
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

const styles = StyleSheet.create({
  overlay: { flex: 1, justifyContent: 'flex-end', backgroundColor: 'rgba(24, 29, 26, 0.36)' },
  sheet: { maxHeight: '93%', paddingBottom: 24, borderTopLeftRadius: 8, borderTopRightRadius: 8, backgroundColor: colors.surface },
  header: { height: 64, paddingHorizontal: 20, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomWidth: StyleSheet.hairlineWidth, borderBottomColor: colors.line },
  title: { color: colors.ink, fontSize: 20, fontWeight: '700', letterSpacing: 0 },
  close: { width: 40, height: 40, alignItems: 'center', justifyContent: 'center' },
  form: { padding: 20, gap: 18 },
  bodyInput: { minHeight: 132, padding: 13, color: colors.ink, borderRadius: 7, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background, fontSize: 15, lineHeight: 23, letterSpacing: 0 },
  field: { gap: 7 },
  label: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  moods: { flexDirection: 'row', justifyContent: 'space-between', gap: 4 },
  moodOption: { alignItems: 'center', gap: 6, paddingVertical: 2 },
  moodDot: { width: 27, height: 27, borderRadius: 14, borderWidth: 3, borderColor: colors.surface },
  moodDotSelected: { borderColor: colors.ink },
  moodText: { color: colors.faint, fontSize: 10, fontWeight: '600', letterSpacing: 0 },
  moodTextSelected: { color: colors.ink },
  metrics: { flexDirection: 'row', gap: 12 },
  timerHint: { marginTop: -10, color: colors.evergreen, fontSize: 11, lineHeight: 17, letterSpacing: 0 },
  metricField: { flex: 1, minWidth: 0 },
  inputWithIcon: { height: 44, paddingHorizontal: 11, flexDirection: 'row', alignItems: 'center', gap: 8, borderRadius: 6, borderWidth: 1, borderColor: colors.line, backgroundColor: colors.background },
  compactInput: { flex: 1, minWidth: 0, paddingVertical: 0, color: colors.ink, fontSize: 13, letterSpacing: 0 },
  compactTextInput: { height: 44, paddingHorizontal: 11, borderRadius: 6, borderWidth: 1, borderColor: colors.line, color: colors.ink, backgroundColor: colors.background, fontSize: 13, letterSpacing: 0 },
  photoPicker: { minHeight: 62, padding: 10, flexDirection: 'row', alignItems: 'center', gap: 11, borderRadius: 7, borderWidth: 1, borderStyle: 'dashed', borderColor: colors.evergreen, backgroundColor: colors.evergreenSoft },
  photoPreview: { width: 42, height: 42, borderRadius: 4, backgroundColor: colors.line },
  photoPickerCopy: { flex: 1, minWidth: 0 },
  photoPickerTitle: { color: colors.ink, fontSize: 13, fontWeight: '700', letterSpacing: 0 },
  photoPickerText: { marginTop: 2, color: colors.muted, fontSize: 11, lineHeight: 16, letterSpacing: 0 },
  photoError: { color: colors.coral, fontSize: 11, lineHeight: 16, letterSpacing: 0 },
  publishRow: { minHeight: 64, padding: 13, flexDirection: 'row', alignItems: 'center', gap: 12, borderRadius: 7, backgroundColor: colors.evergreenSoft },
  publishCopy: { flex: 1, minWidth: 0 },
  publishTitle: { color: colors.ink, fontSize: 14, fontWeight: '700', letterSpacing: 0 },
  publishText: { color: colors.muted, fontSize: 11, lineHeight: 17, marginTop: 3, letterSpacing: 0 },
  submit: { height: 51, marginHorizontal: 20, borderRadius: 6, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.evergreen },
  submitText: { color: colors.surface, fontSize: 15, fontWeight: '700', letterSpacing: 0 },
  disabled: { opacity: 0.35 },
  pressed: { opacity: 0.65 },
});
