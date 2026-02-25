use crate::pipeline::state::Language;

/// Returns the LLM refinement system prompt for the given language.
///
/// These prompts are identical to the Android version (`RefinementPrompt.kt`).
/// They instruct the LLM to clean up speech transcription into polished text.
pub fn for_language(lang: &Language) -> &'static str {
    match lang {
        Language::Chinese => ZH_TW_PROMPT,
        Language::English => EN_PROMPT,
        Language::Japanese => JA_PROMPT,
        Language::Korean => KO_PROMPT,
        Language::French => FR_PROMPT,
        Language::German => DE_PROMPT,
        Language::Spanish => ES_PROMPT,
        Language::Vietnamese => VI_PROMPT,
        Language::Indonesian => ID_PROMPT,
        Language::Thai => TH_PROMPT,
        Language::Auto => MIXED_PROMPT,
    }
}

const ZH_TW_PROMPT: &str = "\
你是一個語音轉文字的編輯助手。請將以下口語內容整理為流暢的書面文字：
1. 移除贅字（嗯、那個、就是、然後、對、呃）
2. 如果說話者中途改口，只保留最終的意思
3. 修正語法但保持原意
4. 適當加入標點符號
5. 不要添加原文沒有的內容
6. 保持繁體中文
只輸出整理後的文字，不要加任何解釋。";

const EN_PROMPT: &str = "\
You are a voice-to-text editor. Clean up the following speech transcription into polished written text:
1. Remove filler words (um, uh, like, you know, I mean, basically, actually, so)
2. If the speaker corrected themselves mid-sentence, keep only the final version
3. Fix grammar while preserving the original meaning
4. Add proper punctuation
5. Do not add content that wasn't in the original speech
Output only the cleaned text, no explanations.";

const JA_PROMPT: &str = "\
あなたは音声テキスト変換の編集アシスタントです。以下の口語内容を整った書き言葉に整理してください：
1. フィラー（えーと、あの、まあ、なんか、ちょっと）を除去
2. 言い直しがある場合は最終的な意味のみ残す
3. 文法を修正し、原意を保持
4. 適切に句読点を追加
5. 原文にない内容を追加しない
整理後のテキストのみ出力し、説明は不要です。";

const KO_PROMPT: &str = "\
당신은 음성-텍스트 변환 편집 도우미입니다. 다음 구어체 내용을 매끄러운 문어체로 정리해 주세요:
1. 군더더기 표현 제거 (음, 그, 뭐, 이제, 약간, 좀)
2. 말하다 고친 부분은 최종 의미만 유지
3. 문법을 교정하되 원래 의미 유지
4. 적절한 문장부호 추가
5. 원문에 없는 내용을 추가하지 않음
정리된 텍스트만 출력하고 설명은 하지 마세요.";

const FR_PROMPT: &str = "\
Vous êtes un éditeur de transcription vocale. Nettoyez la transcription suivante en un texte écrit soigné :
1. Supprimez les mots de remplissage (euh, ben, genre, en fait, du coup, voilà, quoi)
2. Si le locuteur s'est corrigé en cours de phrase, ne gardez que la version finale
3. Corrigez la grammaire en préservant le sens original
4. Ajoutez la ponctuation appropriée
5. N'ajoutez pas de contenu absent du discours original
Produisez uniquement le texte nettoyé, sans explications.";

const DE_PROMPT: &str = "\
Sie sind ein Sprache-zu-Text-Editor. Bereinigen Sie die folgende Sprachtranskription zu einem gepflegten Schrifttext:
1. Füllwörter entfernen (äh, ähm, also, halt, sozusagen, quasi, irgendwie)
2. Bei Selbstkorrekturen nur die endgültige Version beibehalten
3. Grammatik korrigieren, dabei die ursprüngliche Bedeutung bewahren
4. Angemessene Zeichensetzung hinzufügen
5. Keine Inhalte hinzufügen, die nicht im Original vorkommen
Geben Sie nur den bereinigten Text aus, ohne Erklärungen.";

const ES_PROMPT: &str = "\
Eres un editor de transcripción de voz a texto. Limpia la siguiente transcripción en un texto escrito pulido:
1. Elimina muletillas (eh, bueno, o sea, pues, este, como que, básicamente)
2. Si el hablante se corrigió a mitad de frase, conserva solo la versión final
3. Corrige la gramática preservando el significado original
4. Añade la puntuación adecuada
5. No añadas contenido que no estuviera en el discurso original
Produce solo el texto limpio, sin explicaciones.";

const VI_PROMPT: &str = "\
Bạn là trợ lý biên tập chuyển đổi giọng nói thành văn bản. Hãy chỉnh sửa nội dung khẩu ngữ sau thành văn bản viết trau chuốt:
1. Loại bỏ từ đệm (ừm, à, ờ, kiểu, thì, là)
2. Nếu người nói sửa lại giữa chừng, chỉ giữ ý cuối cùng
3. Sửa ngữ pháp nhưng giữ nguyên ý gốc
4. Thêm dấu câu phù hợp
5. Không thêm nội dung không có trong bản gốc
Chỉ xuất văn bản đã chỉnh sửa, không giải thích.";

const ID_PROMPT: &str = "\
Anda adalah editor transkripsi suara ke teks. Bersihkan transkripsi berikut menjadi teks tertulis yang rapi:
1. Hapus kata pengisi (eh, hmm, gitu, kayak, jadi, terus, anu)
2. Jika pembicara mengoreksi diri di tengah kalimat, pertahankan hanya versi akhir
3. Perbaiki tata bahasa dengan mempertahankan makna asli
4. Tambahkan tanda baca yang sesuai
5. Jangan menambahkan konten yang tidak ada dalam ucapan asli
Hasilkan hanya teks yang sudah dibersihkan, tanpa penjelasan.";

const TH_PROMPT: &str = "\
คุณเป็นผู้ช่วยแก้ไขการถอดเสียงเป็นข้อความ กรุณาปรับปรุงเนื้อหาพูดต่อไปนี้ให้เป็นภาษาเขียนที่สละสลวย:
1. ลบคำฟุ่มเฟือย (เอ่อ อ้า แบบ ก็ อะ คือ)
2. หากผู้พูดแก้ไขกลางประโยค ให้เก็บเฉพาะความหมายสุดท้าย
3. แก้ไขไวยากรณ์โดยรักษาความหมายเดิม
4. เพิ่มเครื่องหมายวรรคตอนที่เหมาะสม
5. ไม่เพิ่มเนื้อหาที่ไม่มีในต้นฉบับ
แสดงเฉพาะข้อความที่แก้ไขแล้ว ไม่ต้องอธิบาย";

const MIXED_PROMPT: &str = "\
你是一個語音轉文字的編輯助手。以下口語內容可能包含多種語言混合使用（如中英混合），請保持原本的語言混合方式，整理為流暢的書面文字：
1. 移除各語言的贅字
2. 如果說話者中途改口，只保留最終的意思
3. 修正語法但保持原意和原本的語言選擇
4. 適當加入標點符號
5. 不要把外語強制翻譯成中文
只輸出整理後的文字，不要加任何解釋。";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_zh_tw_prompt_for_chinese() {
        let prompt = for_language(&Language::Chinese);
        assert!(prompt.contains("繁體中文"));
        assert!(prompt.contains("移除贅字"));
    }

    #[test]
    fn should_return_en_prompt_for_english() {
        let prompt = for_language(&Language::English);
        assert!(prompt.contains("filler words"));
        assert!(prompt.contains("no explanations"));
    }

    #[test]
    fn should_return_ja_prompt_for_japanese() {
        let prompt = for_language(&Language::Japanese);
        assert!(prompt.contains("フィラー"));
        assert!(prompt.contains("説明は不要"));
    }

    #[test]
    fn should_return_mixed_prompt_for_auto() {
        let prompt = for_language(&Language::Auto);
        assert!(prompt.contains("多種語言混合"));
        assert!(prompt.contains("不要把外語強制翻譯"));
    }

    #[test]
    fn should_return_korean_prompt_for_korean() {
        let prompt = for_language(&Language::Korean);
        assert!(prompt.contains("군더더기"));
    }

    #[test]
    fn should_return_french_prompt_for_french() {
        let prompt = for_language(&Language::French);
        assert!(prompt.contains("mots de remplissage"));
    }

    #[test]
    fn should_return_german_prompt_for_german() {
        let prompt = for_language(&Language::German);
        assert!(prompt.contains("Füllwörter"));
    }

    #[test]
    fn should_return_spanish_prompt_for_spanish() {
        let prompt = for_language(&Language::Spanish);
        assert!(prompt.contains("muletillas"));
    }

    #[test]
    fn should_return_vietnamese_prompt_for_vietnamese() {
        let prompt = for_language(&Language::Vietnamese);
        assert!(prompt.contains("từ đệm"));
    }

    #[test]
    fn should_return_indonesian_prompt_for_indonesian() {
        let prompt = for_language(&Language::Indonesian);
        assert!(prompt.contains("kata pengisi"));
    }

    #[test]
    fn should_return_thai_prompt_for_thai() {
        let prompt = for_language(&Language::Thai);
        assert!(prompt.contains("คำฟุ่มเฟือย"));
    }

    #[test]
    fn should_not_return_empty_prompts() {
        let all = [
            Language::Auto,
            Language::Chinese,
            Language::English,
            Language::Japanese,
            Language::Korean,
            Language::French,
            Language::German,
            Language::Spanish,
            Language::Vietnamese,
            Language::Indonesian,
            Language::Thai,
        ];
        for lang in &all {
            assert!(!for_language(lang).is_empty());
        }
    }
}
