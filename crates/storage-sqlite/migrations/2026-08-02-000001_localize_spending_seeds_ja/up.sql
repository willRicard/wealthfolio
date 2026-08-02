-- Japanese labels for the system activity taxonomies and their supporting
-- budget/event seed data. Keep stable IDs and keys: rules, assignments, and
-- sync payloads use those identifiers rather than these display labels.

UPDATE budget_groups
SET name = CASE key
  WHEN 'needs' THEN '必要'
  WHEN 'wants' THEN '欲しいもの'
  WHEN 'savings' THEN '貯蓄'
  WHEN 'giving' THEN '寄付'
  WHEN 'personal' THEN '個人'
  WHEN 'other' THEN 'その他'
  ELSE name
END
WHERE is_system = 1;

UPDATE taxonomies
SET name = CASE id
      WHEN 'spending_categories' THEN '支出カテゴリ'
      WHEN 'income_sources' THEN '収入源'
      WHEN 'savings_categories' THEN '貯蓄'
      ELSE name
    END,
    description = CASE id
      WHEN 'spending_categories' THEN '現金の出金や送金を分類するための階層型支出カテゴリです。'
      WHEN 'income_sources' THEN '現金の入金を分類するための収入カテゴリです。'
      WHEN 'savings_categories' THEN '貯蓄・投資に充てた資金です。支出合計には含まれません。'
      ELSE description
    END
WHERE id IN ('spending_categories', 'income_sources', 'savings_categories');

UPDATE taxonomy_categories
SET name = CASE id
  WHEN 'cat_housing' THEN '住居'
  WHEN 'cat_groceries' THEN '食料品'
  WHEN 'cat_food' THEN '食事'
  WHEN 'cat_transport' THEN '交通'
  WHEN 'cat_shopping' THEN '買い物'
  WHEN 'cat_entertainment' THEN 'エンターテインメント'
  WHEN 'cat_health' THEN '健康・ウェルネス'
  WHEN 'cat_bills' THEN '請求書・公共料金'
  WHEN 'cat_personal' THEN '身の回り'
  WHEN 'cat_education' THEN '教育'
  WHEN 'cat_travel' THEN '旅行'
  WHEN 'cat_gifts' THEN '贈答・寄付'
  WHEN 'cat_fees' THEN '手数料・料金'
  WHEN 'cat_other_expense' THEN 'その他の支出'
  WHEN 'cat_savings' THEN '貯蓄'
  WHEN 'cat_housing_rent' THEN '家賃・住宅ローン'
  WHEN 'cat_housing_utilities' THEN '公共料金'
  WHEN 'cat_housing_insurance' THEN '住宅保険'
  WHEN 'cat_housing_maintenance' THEN '維持・修繕'
  WHEN 'cat_housing_furnishing' THEN '家具・インテリア'
  WHEN 'cat_food_restaurants' THEN 'レストラン'
  WHEN 'cat_food_coffee' THEN 'カフェ'
  WHEN 'cat_food_delivery' THEN 'フードデリバリー'
  WHEN 'cat_food_alcohol' THEN 'バー・アルコール'
  WHEN 'cat_transport_gas' THEN 'ガソリン・燃料'
  WHEN 'cat_transport_parking' THEN '駐車場'
  WHEN 'cat_transport_public' THEN '公共交通機関'
  WHEN 'cat_transport_rideshare' THEN '配車・タクシー'
  WHEN 'cat_transport_maintenance' THEN '車両整備'
  WHEN 'cat_transport_insurance' THEN '自動車保険'
  WHEN 'cat_shopping_clothing' THEN '衣類'
  WHEN 'cat_shopping_electronics' THEN '家電・電子機器'
  WHEN 'cat_shopping_home' THEN '生活用品'
  WHEN 'cat_shopping_online' THEN 'オンラインショッピング'
  WHEN 'cat_entertainment_streaming' THEN '動画・音楽配信サービス'
  WHEN 'cat_entertainment_movies' THEN '映画・イベント'
  WHEN 'cat_entertainment_games' THEN 'ゲーム・アプリ'
  WHEN 'cat_entertainment_hobbies' THEN '趣味'
  WHEN 'cat_entertainment_sports' THEN 'スポーツ・レクリエーション'
  WHEN 'cat_health_medical' THEN '医療'
  WHEN 'cat_health_pharmacy' THEN '薬局'
  WHEN 'cat_health_dental' THEN '歯科'
  WHEN 'cat_health_vision' THEN '眼科・視力'
  WHEN 'cat_health_fitness' THEN 'ジム・フィットネス'
  WHEN 'cat_health_insurance' THEN '医療保険'
  WHEN 'cat_bills_phone' THEN '携帯電話'
  WHEN 'cat_bills_internet' THEN 'インターネット'
  WHEN 'cat_bills_subscriptions' THEN 'サブスクリプション'
  WHEN 'cat_bills_software' THEN 'ソフトウェア・サービス'
  WHEN 'cat_fees_bank' THEN '銀行手数料'
  WHEN 'cat_fees_atm' THEN 'ATM手数料'
  WHEN 'cat_fees_interest' THEN '利息・金利'
  WHEN 'cat_fees_late' THEN '延滞料金'
  WHEN 'cat_savings_emergency' THEN '緊急資金'
  WHEN 'cat_savings_retirement' THEN '老後資金'
  WHEN 'cat_savings_investments' THEN '投資積立'
  WHEN 'cat_savings_short_term' THEN '短期貯蓄'
  WHEN 'cat_savings_education' THEN '教育資金'
  WHEN 'cat_savings_charitable' THEN '寄付用積立'
  WHEN 'cat_income_employment' THEN '雇用収入'
  WHEN 'cat_income_selfemploy' THEN '自営業'
  WHEN 'cat_income_investment' THEN '投資収入'
  WHEN 'cat_income_other' THEN 'その他の収入'
  WHEN 'cat_income_salary' THEN '給与'
  WHEN 'cat_income_bonus' THEN '賞与'
  WHEN 'cat_income_commission' THEN '歩合・コミッション'
  WHEN 'cat_income_freelance' THEN 'フリーランス'
  WHEN 'cat_income_business' THEN '事業収入'
  WHEN 'cat_income_dividends' THEN '配当'
  WHEN 'cat_income_interest' THEN '利息'
  WHEN 'cat_income_rental' THEN '賃貸収入'
  WHEN 'cat_income_capital_gains' THEN '売却益'
  WHEN 'cat_income_gifts' THEN '受取贈与'
  WHEN 'cat_income_refunds' THEN '返金'
  WHEN 'cat_income_reimbursements' THEN '立替精算'
  WHEN 'cat_income_tax_refund' THEN '税金還付'
  ELSE name
END
WHERE taxonomy_id IN ('spending_categories', 'income_sources', 'savings_categories');

UPDATE spending_event_types
SET name = CASE key
  WHEN 'travel' THEN '旅行'
  WHEN 'holiday' THEN '休暇'
  WHEN 'business' THEN '仕事'
  WHEN 'education' THEN '教育'
  WHEN 'medical' THEN '医療'
  WHEN 'special_occasion' THEN '特別な行事'
  WHEN 'other' THEN 'その他'
  ELSE name
END
WHERE key IN ('travel', 'holiday', 'business', 'education', 'medical', 'special_occasion', 'other');
